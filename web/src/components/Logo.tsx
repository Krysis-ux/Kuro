/**
 * The Kuro mark: a rounded k whose upper arm is a dot.
 *
 * Drawn on a 100-unit grid shared with `public/favicon.svg`, the macOS app's
 * `KuroGlyph` and `packaging/macos/icon.svg`. Change them together.
 */
export function Logo({ size = 24, className }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 100 100"
      className={className}
      role="img"
      aria-label="Kuro"
    >
      <path
        d="M28 20 V80 M42 52 L70 80"
        fill="none"
        stroke="currentColor"
        strokeWidth={15}
        strokeLinecap="round"
      />
      <circle cx="66" cy="32" r="13" fill="currentColor" />
    </svg>
  )
}
