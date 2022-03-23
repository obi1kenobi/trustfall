use std::rc::Rc;

use itertools::Itertools;
use octorust::types::ContentFile;
use yaml_rust::YamlLoader;

use crate::token::{ActionsImportedStep, ActionsJob, ActionsRunStep, Token};

pub(crate) fn get_jobs_in_workflow_file(
    content: Rc<ContentFile>,
) -> Box<dyn Iterator<Item = Token>> {
    let file_content =
        String::from_utf8(base64::decode(content.content.replace('\n', "")).unwrap()).unwrap();
    let docs = match YamlLoader::load_from_str(file_content.as_str()) {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "invalid yaml in workflow file {}: {}",
                content.path.as_str(),
                e
            );
            return Box::new(std::iter::empty());
        }
    };

    Box::new(
        docs.into_iter()
            .flat_map(move |workflow_yaml| -> Box<dyn Iterator<Item = Token>> {
                let jobs_element = workflow_yaml["jobs"].clone();
                if jobs_element.is_badvalue() {
                    eprintln!(
                        "invalid yaml, couldn't find 'jobs' in workflow file {}:\n{}",
                        content.path.as_str(),
                        file_content
                    );
                }

                let jobs = match jobs_element.into_hash() {
                    None => {
                        eprintln!(
                            "unexpected 'jobs' section in workflow file {}:\n{}",
                            content.path.as_str(),
                            file_content
                        );
                        return Box::new(std::iter::empty());
                    }
                    Some(jobs) => jobs,
                };

                Box::new(jobs.into_iter().filter_map(|(name, job_content)| {
                    let name = name.as_str()?.to_string();
                    job_content.as_hash()?;

                    let runs_on = job_content["runs-on"].as_str().map(|x| x.to_string());

                    Some(Token::GitHubActionsJob(Rc::new(ActionsJob::new(
                        job_content,
                        name,
                        runs_on,
                    ))))
                }))
            }),
    )
}

pub(crate) fn get_steps_in_job(job: Rc<ActionsJob>) -> Box<dyn Iterator<Item = Token>> {
    let steps = job.yaml["steps"].clone();
    if steps.is_badvalue() || !steps.is_array() {
        eprintln!(
            "invalid yaml, no 'steps' array in workflow job yaml: {:?}",
            job
        );
        return Box::new(std::iter::empty());
    }

    Box::new(steps.into_iter().filter_map(move |element| {
        element.as_hash()?;

        let name = element["name"].as_str().map(|x| x.to_string());
        let uses = element["uses"].as_str();
        let run_element = &element["run"];
        let run = if run_element.is_array() {
            run_element.as_vec().and_then(|vec| {
                vec.iter()
                    .map(|elem| elem.as_str().map(|x| x.to_string()).ok_or(elem))
                    .try_collect()
                    .ok()
            })
        } else {
            run_element.as_str().map(|r| vec![r.to_string()])
        };

        match (uses, run) {
            (Some(uses), None) => {
                // 'uses' key -> imported step
                let uses = uses.to_string();
                Some(ActionsImportedStep::new(element, name, uses).into())
            }
            (None, Some(run)) => {
                // explicit run commands -> run step
                Some(ActionsRunStep::new(element, name, run).into())
            }
            _ => {
                eprintln!(
                    "unexpected job step with name {:?} uses {:?} run {:?}: {:?}",
                    name, uses, run_element, job
                );
                None
            }
        }
    }))
}
