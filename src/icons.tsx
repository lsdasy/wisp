import type { ReactNode } from "react";

function Svg({ size = 16, children }: { size?: number; children: ReactNode }) {
  return (
    <svg
      aria-hidden="true"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {children}
    </svg>
  );
}

export const Icon = {
  search: (s?: number) => (
    <Svg size={s ?? 20}>
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-3.5-3.5" />
    </Svg>
  ),
  power: () => (
    <Svg>
      <path d="M12 2v10" />
      <path d="M18.4 6.6a9 9 0 1 1-12.8 0" />
    </Svg>
  ),
  command: () => (
    <Svg>
      <path d="M9 6a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3z" />
    </Svg>
  ),
  file: () => (
    <Svg>
      <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" />
      <path d="M14 3v5h5" />
    </Svg>
  ),
  folder: () => (
    <Svg>
      <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    </Svg>
  ),
  shell: () => (
    <Svg>
      <path d="m5 8 4 4-4 4" />
      <path d="M12 17h7" />
    </Svg>
  ),
  puzzle: () => (
    <Svg>
      <path d="M10 3h4v3a2 2 0 1 0 4 0V3h3v7h-3a2 2 0 1 0 0 4h3v7h-7v-3a2 2 0 1 0-4 0v3H3v-7h3a2 2 0 1 0 0-4H3V3z" />
    </Svg>
  ),
  arrow: () => (
    <Svg size={22}>
      <path d="M5 12h14M13 6l6 6-6 6" />
    </Svg>
  ),
  warning: () => (
    <Svg size={15}>
      <path d="M12 3 2 21h20z" />
      <path d="M12 10v4M12 17.5v.5" />
    </Svg>
  ),
  star: (filled: boolean) => (
    <svg aria-hidden="true" width="13" height="13" viewBox="0 0 24 24" fill={filled ? "currentColor" : "none"} stroke="currentColor" strokeWidth={2} strokeLinejoin="round">
      <path d="m12 3 2.8 5.7 6.2.9-4.5 4.4 1 6.2L12 17.3 6.5 20.2l1-6.2L3 9.6l6.2-.9z" />
    </svg>
  ),
  logo: (s?: number) => (
    <Svg size={s ?? 16}>
      <path d="M12 2.5v4M12 17.5v4M2.5 12h4M17.5 12h4" />
      <circle cx="12" cy="12" r="3" />
    </Svg>
  ),
  back: () => (
    <Svg>
      <path d="M15 18l-6-6 6-6" />
    </Svg>
  ),
  close: () => (
    <Svg size={14}>
      <path d="M6 6l12 12M18 6 6 18" />
    </Svg>
  ),
};

export function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="kbd">{children}</kbd>;
}
