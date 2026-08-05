import { QueryClient } from '@tanstack/react-query'

/**
 * The one query cache, reachable from outside React.
 *
 * It used to be built inside `main.tsx` and handed down through the provider,
 * which is fine for components and useless for anything that outlives one. A
 * turn keeps running after the page that started it has been navigated away
 * from, and when it finishes it still has to replace the optimistic message
 * with the stored one — from a module, with no component and no hook in scope.
 */
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Everything is on loopback, so refetching is cheap; but constant
      // refetching while reading a long answer is distracting.
      refetchOnWindowFocus: false,
      staleTime: 5_000,
      retry: 1,
    },
  },
})
