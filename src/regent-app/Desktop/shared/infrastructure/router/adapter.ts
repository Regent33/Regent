// Next-compat seam: mirrors the `next/navigation` hook shapes on top of
// react-router-dom so feature code stays router-agnostic and existing files
// only swap their import specifier (from 'next/navigation' to this module).
import { useMemo } from 'react';
import { useLocation, useNavigate, useSearchParams as useRRSearchParams } from 'react-router-dom';

export function useRouter(): { push(href: string): void; replace(href: string): void } {
  const navigate = useNavigate();
  return useMemo(
    () => ({
      push: (href: string) => navigate(href),
      replace: (href: string) => navigate(href, { replace: true }),
    }),
    [navigate],
  );
}

export function usePathname(): string {
  return useLocation().pathname;
}

export function useSearchParams(): URLSearchParams {
  return useRRSearchParams()[0];
}
