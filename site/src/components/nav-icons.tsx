import type { ReactNode } from "react";

type IconProps = { children: ReactNode };

function Icon({ children }: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {children}
    </svg>
  );
}

export function DiscordIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M19.27 5.33A16.8 16.8 0 0 0 15.1 4l-.2.37a15.4 15.4 0 0 1 3.85 1.2 16.3 16.3 0 0 0-12.5 0A12 12 0 0 1 10.1 4.37L9.9 4a16.7 16.7 0 0 0-4.17 1.33C2.4 9.02 1.6 12.62 2 16.18A17 17 0 0 0 7.3 18.6l.5-.68a11 11 0 0 1-1.68-.8l.4-.3c3.28 1.5 6.84 1.5 10.08 0l.4.3c-.54.32-1.1.6-1.68.8l.5.68a16.9 16.9 0 0 0 5.3-2.42c.47-4.14-.8-7.7-2.85-10.85ZM9.2 14.15c-.8 0-1.46-.75-1.46-1.66s.64-1.66 1.46-1.66 1.48.75 1.46 1.66-.64 1.66-1.46 1.66Zm5.6 0c-.8 0-1.46-.75-1.46-1.66s.64-1.66 1.46-1.66 1.48.75 1.46 1.66-.64 1.66-1.46 1.66Z" />
    </svg>
  );
}

export function GitHubIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M12 2a10 10 0 0 0-3.16 19.49c.5.09.68-.22.68-.48v-1.7c-2.78.6-3.37-1.18-3.37-1.18-.45-1.16-1.1-1.47-1.1-1.47-.9-.62.07-.6.07-.6 1 .07 1.53 1.04 1.53 1.04.9 1.52 2.34 1.08 2.91.83.09-.65.35-1.08.63-1.33-2.22-.25-4.55-1.11-4.55-4.95 0-1.09.39-1.99 1.03-2.69-.1-.25-.45-1.27.1-2.65 0 0 .84-.27 2.75 1.03a9.6 9.6 0 0 1 5 0c1.91-1.3 2.75-1.03 2.75-1.03.55 1.38.2 2.4.1 2.65.64.7 1.03 1.6 1.03 2.69 0 3.85-2.34 4.7-4.57 4.95.36.31.68.92.68 1.86v2.76c0 .26.18.58.69.48A10 10 0 0 0 12 2Z" />
    </svg>
  );
}

export function DownloadIcon() {
  return (
    <Icon>
      <path d="M12 3v12" />
      <path d="m7 11 5 5 5-5" />
      <path d="M5 21h14" />
    </Icon>
  );
}

export function CloudIcon() {
  return (
    <Icon>
      <path d="M7 18h10.2a3.8 3.8 0 0 0 .3-7.6 6 6 0 0 0-11.5 1.6A3.4 3.4 0 0 0 7 18Z" />
    </Icon>
  );
}

export function SparkleIcon() {
  return (
    <Icon>
      <path d="M12 3.2 13.1 8.7 18.5 10 13.1 11.3 12 16.8 10.9 11.3 5.5 10 10.9 8.7 12 3.2Z" />
      <path d="M18 15.2 18.5 17l1.8.5-1.8.5-.5 1.8-.5-1.8-1.8-.5 1.8-.5.5-1.8Z" />
    </Icon>
  );
}
