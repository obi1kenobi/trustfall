import { useNavigate, useLocation } from 'react-router-dom';
import { QueryParamAdapter, QueryParamAdapterComponent } from 'use-query-params';

/**
 * Query Param Adapter for use-query-params that uses URL fragments
 * instead of query params, which allows us to bypass query param length limits,
 * since fragments are not sent to the server.
 */
export const QueryFragmentAdapter: QueryParamAdapterComponent = ({ children }) => {
  const navigate = useNavigate();
  const location = useLocation();

  const adapter: QueryParamAdapter = {
    replace(location) {
      navigate(`#${location.search ?? '?'}`, {
        replace: true,
        state: location.state,
      });
    },
    push(location) {
      navigate(`#${location.search ?? '?'}`, {
        replace: false,
        state: location.state,
      });
    },
    get location() {
      return {
        ...location,
        search: location.hash.slice(1), // Remove leading '#'
      };
    },
  };

  return children(adapter);
};
