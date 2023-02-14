use std::rc::Rc;

use itertools::Itertools;
use octorust::types::ContentFile;
use trustfall_core::interpreter::VertexIterator;
use yaml_rust::{Yaml, YamlLoader};

use crate::vertex::{ActionsImportedStep, ActionsJob, ActionsRunStep, Vertex};

pub(crate) fn get_jobs_in_workflow_file(
    content: Rc<ContentFile>,
) -> VertexIterator<'static, Vertex> {
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
            .flat_map(move |workflow_yaml| -> VertexIterator<'static, Vertex> {
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

                    Some(Vertex::GitHubActionsJob(Rc::new(ActionsJob::new(
                        job_content,
                        name,
                        runs_on,
                    ))))
                }))
            }),
    )
}

pub(crate) fn get_steps_in_job(job: Rc<ActionsJob>) -> VertexIterator<'static, Vertex> {
    let steps = job.yaml["steps"].clone();
    if steps.is_badvalue() || !steps.is_array() {
        eprintln!("invalid yaml, no 'steps' array in workflow job yaml: {job:?}",);
        return Box::new(std::iter::empty());
    }

    Box::new(steps.into_iter().filter_map(move |element| {
        element.as_hash()?;

        let name = element["name"].as_str().map(|x| x.to_string());
        let uses = element["uses"].as_str();
        let run_element = &element["run"];
        let run = run_element
            .as_str()
            .map(|r| r.trim().split('\n').map(|x| x.to_string()).collect_vec());

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
                    "unexpected job step with name {name:?} uses {uses:?} run {run_element:?}: {job:?}",
                );
                None
            }
        }
    }))
}

pub(crate) fn get_env_for_run_step(step: Rc<ActionsRunStep>) -> VertexIterator<'static, Vertex> {
    let step_hash = match step.yaml.clone() {
        Yaml::Hash(h) => h,
        _ => unreachable!(),
    };
    let env_key = Yaml::String("env".to_string());
    let env_hash = match step_hash.get(&env_key) {
        Some(Yaml::Hash(h)) => h.clone(),
        Some(unexpected) => {
            eprintln!(
                "unexpected value {:?} for 'env' key: {:?}",
                unexpected, step.yaml
            );
            return Box::new(std::iter::empty());
        }
        None => {
            // No "env" key for this run step.
            return Box::new(std::iter::empty());
        }
    };

    Box::new(
        env_hash
            .into_iter()
            .filter_map(move |(key_yaml, value_yaml)| {
                let key = key_yaml.as_str()?.to_string();
                let value = match value_yaml {
                    Yaml::Real(s) | Yaml::String(s) => s,
                    Yaml::Integer(i) => i.to_string(),
                    Yaml::Boolean(b) => b.to_string(),
                    _ => {
                        eprintln!("unexpected value for env key {key}: {value_yaml:?}");
                        return None;
                    }
                };

                Some(Vertex::NameValuePair(Rc::from((key, value))))
            }),
    )
}
