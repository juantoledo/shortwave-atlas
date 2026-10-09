// A country flag, or a small globe for stations without one (clandestine, international).

import { flagUrl } from '../catalog/flags';

export function Flag({ flag, title }: { flag: string | null; title: string }) {
  const url = flagUrl(flag);
  if (url) return <img className="flag" src={url} alt={title} title={title} loading="lazy" decoding="async" />;
  return (
    <svg className="flag none" viewBox="0 0 16 12" role="img" aria-label={title}>
      <title>{title}</title>
      <circle cx="8" cy="6" r="4.6" />
      <path d="M3.4 6h9.2M8 1.4c-2.6 2.9-2.6 6.3 0 9.2M8 1.4c2.6 2.9 2.6 6.3 0 9.2" />
    </svg>
  );
}
