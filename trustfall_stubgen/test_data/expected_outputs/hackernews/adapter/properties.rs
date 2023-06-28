use trustfall::{FieldValue, provider::{ContextIterator, ContextOutcomeIterator, ResolveInfo}};

use super::vertex::Vertex;

pub(super) fn resolve_comment_property<'a>(
    contexts: ContextIterator<'a, Vertex>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, Vertex, FieldValue> {
    match property_name {
        "byUsername" => {
            todo!("implement property 'byUsername' in fn `resolve_comment_property()`")
        }
        "id" => todo!("implement property 'id' in fn `resolve_comment_property()`"),
        "textHtml" => {
            todo!("implement property 'textHtml' in fn `resolve_comment_property()`")
        }
        "textPlain" => {
            todo!("implement property 'textPlain' in fn `resolve_comment_property()`")
        }
        "unixTime" => {
            todo!("implement property 'unixTime' in fn `resolve_comment_property()`")
        }
        "url" => todo!("implement property 'url' in fn `resolve_comment_property()`"),
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'Comment'"
            )
        }
    }
}

pub(super) fn resolve_item_property<'a>(
    contexts: ContextIterator<'a, Vertex>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, Vertex, FieldValue> {
    match property_name {
        "id" => todo!("implement property 'id' in fn `resolve_item_property()`"),
        "unixTime" => {
            todo!("implement property 'unixTime' in fn `resolve_item_property()`")
        }
        "url" => todo!("implement property 'url' in fn `resolve_item_property()`"),
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'Item'"
            )
        }
    }
}

pub(super) fn resolve_job_property<'a>(
    contexts: ContextIterator<'a, Vertex>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, Vertex, FieldValue> {
    match property_name {
        "id" => todo!("implement property 'id' in fn `resolve_job_property()`"),
        "score" => todo!("implement property 'score' in fn `resolve_job_property()`"),
        "submittedUrl" => {
            todo!("implement property 'submittedUrl' in fn `resolve_job_property()`")
        }
        "title" => todo!("implement property 'title' in fn `resolve_job_property()`"),
        "unixTime" => {
            todo!("implement property 'unixTime' in fn `resolve_job_property()`")
        }
        "url" => todo!("implement property 'url' in fn `resolve_job_property()`"),
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'Job'"
            )
        }
    }
}

pub(super) fn resolve_story_property<'a>(
    contexts: ContextIterator<'a, Vertex>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, Vertex, FieldValue> {
    match property_name {
        "byUsername" => {
            todo!("implement property 'byUsername' in fn `resolve_story_property()`")
        }
        "id" => todo!("implement property 'id' in fn `resolve_story_property()`"),
        "score" => todo!("implement property 'score' in fn `resolve_story_property()`"),
        "submittedUrl" => {
            todo!("implement property 'submittedUrl' in fn `resolve_story_property()`")
        }
        "textHtml" => {
            todo!("implement property 'textHtml' in fn `resolve_story_property()`")
        }
        "textPlain" => {
            todo!("implement property 'textPlain' in fn `resolve_story_property()`")
        }
        "title" => todo!("implement property 'title' in fn `resolve_story_property()`"),
        "unixTime" => {
            todo!("implement property 'unixTime' in fn `resolve_story_property()`")
        }
        "url" => todo!("implement property 'url' in fn `resolve_story_property()`"),
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'Story'"
            )
        }
    }
}

pub(super) fn resolve_user_property<'a>(
    contexts: ContextIterator<'a, Vertex>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, Vertex, FieldValue> {
    match property_name {
        "aboutHtml" => {
            todo!("implement property 'aboutHtml' in fn `resolve_user_property()`")
        }
        "aboutPlain" => {
            todo!("implement property 'aboutPlain' in fn `resolve_user_property()`")
        }
        "id" => todo!("implement property 'id' in fn `resolve_user_property()`"),
        "karma" => todo!("implement property 'karma' in fn `resolve_user_property()`"),
        "unixCreatedAt" => {
            todo!("implement property 'unixCreatedAt' in fn `resolve_user_property()`")
        }
        "url" => todo!("implement property 'url' in fn `resolve_user_property()`"),
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'User'"
            )
        }
    }
}

pub(super) fn resolve_webpage_property<'a>(
    contexts: ContextIterator<'a, Vertex>,
    property_name: &str,
    _resolve_info: &ResolveInfo,
) -> ContextOutcomeIterator<'a, Vertex, FieldValue> {
    match property_name {
        "url" => todo!("implement property 'url' in fn `resolve_webpage_property()`"),
        _ => {
            unreachable!(
                "attempted to read unexpected property '{property_name}' on type 'Webpage'"
            )
        }
    }
}
