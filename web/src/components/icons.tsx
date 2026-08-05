/**
 * Line icons, drawn on a 24-unit grid at a consistent 1.6 stroke weight.
 *
 * Hand-rolled rather than pulled from a library: there are few enough of them
 * that a dependency would cost more than it saves, and a single stroke weight
 * keeps the interface visually quiet.
 */

type IconProps = { size?: number; className?: string }

function Svg({ size = 16, className, children }: IconProps & { children: React.ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      {children}
    </svg>
  )
}

export const PlusIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M12 5v14M5 12h14" />
  </Svg>
)

export const SendIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M12 19V5M5 12l7-7 7 7" />
  </Svg>
)

export const StopIcon = (props: IconProps) => (
  <Svg {...props}>
    <rect x="7" y="7" width="10" height="10" rx="1.5" fill="currentColor" stroke="none" />
  </Svg>
)

export const ChatIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M20 15a2 2 0 0 1-2 2H8l-4 4V5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2z" />
  </Svg>
)

export const CubeIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" />
    <path d="m3.3 7 8.7 5 8.7-5M12 22V12" />
  </Svg>
)

export const PlugIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M9 2v6M15 2v6M6 8h12v3a6 6 0 0 1-12 0zM12 17v5" />
  </Svg>
)

export const SettingsIcon = (props: IconProps) => (
  <Svg {...props}>
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
  </Svg>
)

export const GlobeIcon = (props: IconProps) => (
  <Svg {...props}>
    <circle cx="12" cy="12" r="9" />
    <path d="M3 12h18M12 3a15 15 0 0 1 0 18 15 15 0 0 1 0-18z" />
  </Svg>
)

export const SearchIcon = (props: IconProps) => (
  <Svg {...props}>
    <circle cx="11" cy="11" r="7" />
    <path d="m20 20-3.5-3.5" />
  </Svg>
)

export const TrashIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M3 6h18M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
  </Svg>
)

export const ChevronIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="m6 9 6 6 6-6" />
  </Svg>
)

export const CloudIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z" />
  </Svg>
)

export const PaperclipIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48" />
  </Svg>
)

export const FolderIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
  </Svg>
)

export const SparkIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M12 3v4M12 17v4M3 12h4M17 12h4M5.6 5.6l2.8 2.8M15.6 15.6l2.8 2.8M18.4 5.6l-2.8 2.8M8.4 15.6l-2.8 2.8" />
  </Svg>
)

export const GitHubIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M9 19c-5 1.5-5-2.5-7-3m14 6v-3.87a3.37 3.37 0 0 0-.94-2.61c3.14-.35 6.44-1.54 6.44-7A5.44 5.44 0 0 0 20 4.77 5.07 5.07 0 0 0 19.91 1S18.73.65 16 2.48a13.38 13.38 0 0 0-7 0C6.27.65 5.09 1 5.09 1A5.07 5.07 0 0 0 5 4.77a5.44 5.44 0 0 0-1.5 3.78c0 5.42 3.3 6.61 6.44 7A3.37 3.37 0 0 0 9 18.13V22" />
  </Svg>
)

export const InfoIcon = (props: IconProps) => (
  <Svg {...props}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 16v-4M12 8h.01" />
  </Svg>
)

export const DownloadIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3" />
  </Svg>
)

export const CheckIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="m4 12 5 5L20 6" />
  </Svg>
)

export const ToolIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M14.7 6.3a4 4 0 0 1 5 5L20 12l-8 8-4-4 8-8zM7 14l-4 4 3 3 4-4" />
  </Svg>
)

export const BrainIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M9 3a3 3 0 0 0-3 3 3 3 0 0 0-1 5.8V15a3 3 0 0 0 3 3h1v3M15 3a3 3 0 0 1 3 3 3 3 0 0 1 1 5.8V15a3 3 0 0 1-3 3h-1v3" />
  </Svg>
)

export const ImageIcon = (props: IconProps) => (
  <Svg {...props}>
    <rect x="3" y="4" width="18" height="16" rx="2" />
    <circle cx="8.5" cy="9.5" r="1.5" />
    <path d="m21 16-5-5-6 6" />
  </Svg>
)

export const FileIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M14 3v5h5M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" />
  </Svg>
)

export const MicIcon = (props: IconProps) => (
  <Svg {...props}>
    <rect x="9" y="2" width="6" height="11" rx="3" />
    <path d="M5 11a7 7 0 0 0 14 0M12 18v4" />
  </Svg>
)

export const VideoIcon = (props: IconProps) => (
  <Svg {...props}>
    <rect x="2" y="6" width="13" height="12" rx="2" />
    <path d="m15 11 7-4v10l-7-4z" />
  </Svg>
)

export const RefreshIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M21 12a9 9 0 1 1-3-6.7M21 4v5h-5" />
  </Svg>
)

export const KeyIcon = (props: IconProps) => (
  <Svg {...props}>
    <circle cx="8" cy="15" r="4" />
    <path d="m11 12 8-8 3 3-2 2-2-2M16 7l3 3" />
  </Svg>
)

export const ExternalIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5" />
  </Svg>
)

export const StoreIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M3 9h18l-1.5-5.2A1 1 0 0 0 18.5 3h-13a1 1 0 0 0-1 .8zM4 9v11a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1V9M9 21v-6h6v6" />
  </Svg>
)

export const PowerIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M12 3v9M18.4 6.6a9 9 0 1 1-12.8 0" />
  </Svg>
)

export const CopyIcon = (props: IconProps) => (
  <Svg {...props}>
    <rect x="9" y="9" width="11" height="11" rx="2" />
    <path d="M5 15V5a1 1 0 0 1 1-1h9" />
  </Svg>
)

export const PencilIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M4 20h4L19 9a2.1 2.1 0 0 0-3-3L5 17zM14 6l4 4" />
  </Svg>
)

/** Angle brackets, for the coding surface. */
export const BracesIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M9 4H7.5A2.5 2.5 0 0 0 5 6.5v3A2.5 2.5 0 0 1 2.5 12 2.5 2.5 0 0 1 5 14.5v3A2.5 2.5 0 0 0 7.5 20H9" />
    <path d="M15 4h1.5A2.5 2.5 0 0 1 19 6.5v3a2.5 2.5 0 0 0 2.5 2.5 2.5 2.5 0 0 0-2.5 2.5v3a2.5 2.5 0 0 1-2.5 2.5H15" />
  </Svg>
)

/** Two paths diverging from one, for branching a conversation. */
export const BranchIcon = (props: IconProps) => (
  <Svg {...props}>
    <circle cx="6" cy="5" r="2.2" />
    <circle cx="6" cy="19" r="2.2" />
    <circle cx="18" cy="12" r="2.2" />
    <path d="M6 7.2v9.6M8.2 17.6A8 8 0 0 0 15.9 12.6" />
  </Svg>
)

export const GiftIcon = (props: IconProps) => (
  <Svg {...props}>
    <rect x="3" y="8" width="18" height="4" rx="1" />
    <path d="M5 12v8a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-8M12 8v13" />
    <path d="M12 8C12 8 10.5 3 8 3a2.5 2.5 0 0 0 0 5M12 8c0 0 1.5-5 4-5a2.5 2.5 0 0 1 0 5" />
  </Svg>
)

export const TerminalIcon = (props: IconProps) => (
  <Svg {...props}>
    <rect x="3" y="4" width="18" height="16" rx="2" />
    <path d="M7 9l3 3-3 3M13 15h4" />
  </Svg>
)

export const PlayIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M6 4l14 8-14 8V4z" />
  </Svg>
)

export const EyeIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7-10-7-10-7z" />
    <circle cx="12" cy="12" r="3" />
  </Svg>
)

export const SlidersIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M4 6h10M18 6h2M4 12h4M12 12h8M4 18h10M18 18h2" />
    <circle cx="16" cy="6" r="2" />
    <circle cx="10" cy="12" r="2" />
    <circle cx="16" cy="18" r="2" />
  </Svg>
)

export const BoltIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M13 2 4 14h7l-1 8 9-12h-7l1-8z" />
  </Svg>
)

export const CloseIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M18 6 6 18M6 6l12 12" />
  </Svg>
)

/** A checklist, for Plan mode. */
export const ListIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M9 6h11M9 12h11M9 18h11M4 6h.01M4 12h.01M4 18h.01" />
  </Svg>
)

/** A shield with a line through it, for the mode with no allowlist. */
export const ShieldOffIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M19.7 14A8.4 8.4 0 0 0 20 12V5l-8-3-3.3 1.2M6.3 6.3 4 5v7c0 5 8 10 8 10a20 20 0 0 0 4.6-3.3" />
    <path d="M2 2l20 20" />
  </Svg>
)

/** Bars, for the panel that counts what has been spent. */
export const ChartIcon = (props: IconProps) => (
  <Svg {...props}>
    <path d="M4 20V10M10 20V4M16 20v-7M22 20H2" />
  </Svg>
)

/**
 * A pane, with the side rail marked.
 *
 * The standard icon for "show or hide the panel beside this" — the same shape
 * every editor uses for it. It reads as a layout rather than as a direction,
 * which is what the sidebar toggle wants: a chevron promises movement one way
 * and then has to flip, and a flipping chevron is the part that never quite
 * looks right.
 */
export const PanelIcon = (props: IconProps) => (
  <Svg {...props}>
    <rect x="3" y="4" width="18" height="16" rx="2" />
    <path d="M10 4v16" />
  </Svg>
)
