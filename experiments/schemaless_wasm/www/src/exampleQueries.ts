/* eslint-disable @typescript-eslint/ban-ts-comment */
// @ts-ignore
import actions_in_repos_with_min_hn_pts from '../../../schemaless/example_queries/actions_in_repos_with_min_hn_pts.graphql';
// @ts-ignore
import crates_io_github_actions from '../../../schemaless/example_queries/crates_io_github_actions.graphql';
// @ts-ignore
import hackernews_patio11_own_post_comments from '../../../schemaless/example_queries/hackernews_patio11_own_post_comments.graphql';
// @ts-ignore
import repos_with_min_hackernews_points from '../../../schemaless/example_queries/repos_with_min_hackernews_points.graphql';
/* eslint-enable @typescript-eslint/ban-ts-comment */

const EXAMPLE_QUERY_MAP = {
    actions_in_repos_with_min_hn_pts: {
        label: 'GitHub Actions in Repos on HackerNews',
        query: actions_in_repos_with_min_hn_pts,
    },
    crates_io_github_actions: {
        label: 'GitHub Actions in the Most Downloaded Crates',
        query: crates_io_github_actions,
    },
    hackernews_patio11_own_post_comments: {
        label: 'Own-Post Comments by User patio11',
        query: hackernews_patio11_own_post_comments,
    },
    repos_with_min_hackernews_points: {
        label: 'GitHub Repos in HackerNews Posts with Min Points',
        query: repos_with_min_hackernews_points,
    },
} as const;

export { EXAMPLE_QUERY_MAP };
