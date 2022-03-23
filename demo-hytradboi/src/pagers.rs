use consecrates::{api::Crate, Query, Sorting};
use octorust::types::Workflow;
use tokio::runtime::Runtime;

use crate::{util::{Pager, PagerOutput}, token::Token};

pub(crate) struct CratesPager<'a> {
    client: &'a consecrates::Client,
}

impl<'a> CratesPager<'a> {
    pub(crate) fn new(client: &'a consecrates::Client) -> Self {
        Self { client }
    }
}

impl<'a> Pager for CratesPager<'a> {
    type Item = Crate;

    fn get_page(&mut self, page: usize) -> PagerOutput<Self::Item> {
        let per_page = 100;
        match self.client.get_crates(Query {
            page: Some(page),
            sort: Some(Sorting::RecentDownloads),
            per_page: Some(per_page),
            ..Default::default()
        }) {
            Ok(c) => {
                if c.crates.is_empty() {
                    PagerOutput::None
                } else if c.crates.len() == per_page {
                    PagerOutput::Page(c.crates.into_iter())
                } else {
                    PagerOutput::KnownFinalPage(c.crates.into_iter())
                }
            }
            Err(e) => {
                eprintln!(
                    "Got an error while getting most downloaded crates page {}: {}",
                    page, e
                );
                PagerOutput::None
            }
        }
    }
}

pub(crate) struct WorkflowsPager<'a> {
    actions: octorust::actions::Actions,
    repo_token: Token,
    runtime: &'a Runtime,
}

impl<'a> WorkflowsPager<'a> {
    pub(crate) fn new(client: octorust::Client, repo_token: Token, runtime: &'a Runtime) -> Self {
        Self {
            actions: octorust::actions::Actions::new(client),
            repo_token,
            runtime,
        }
    }
}

impl<'a> Pager for WorkflowsPager<'a> {
    type Item = Workflow;

    fn get_page(&mut self, page: usize) -> PagerOutput<Self::Item> {
        let per_page = 100;
        let repo = self.repo_token.as_github_repository().unwrap();
        let owner = repo.owner.as_ref().unwrap().login.as_str();
        let repo_name = repo.name.as_str();

        println!("getting workflows page {} for {}/{}", page, owner, repo_name);

        match self.runtime.block_on(self.actions.list_repo_workflows(
            owner,
            repo_name,
            per_page,
            page as i64,
        )) {
            Ok(response) => {
                println!("{:?}", response);
                if response.workflows.is_empty() {
                    PagerOutput::None
                } else if response.workflows.len() == per_page as usize {
                    PagerOutput::Page(response.workflows.into_iter())
                } else {
                    PagerOutput::KnownFinalPage(response.workflows.into_iter())
                }
            }
            Err(e) => {
                eprintln!(
                    "Got an error while getting repo {} workflows page {}: {}",
                    repo.full_name.as_str(),
                    page,
                    e
                );
                PagerOutput::None
            }
        }
    }
}
