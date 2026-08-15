import { useId } from 'react'

export function Logo({ size = 24, className }: { size?: number; className?: string }) {
  const maskId = useId()

  const front = 'M50 50 C36 40 34 18 50 6 C66 18 64 40 50 50 Z'
  const back = 'M50 50 C39 42 38 25 50 15 C62 25 61 42 50 50 Z'
  const angles = [0, 45, 90, 135, 180, 225, 270, 315]

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 100 100"
      className={className}
      role="img"
      aria-label="Kuro"
    >
      <mask id={maskId}>
        <rect width="100" height="100" fill="black" />
        <g fill="white">
          <g opacity="0.55">
            {angles.map((angle) => (
              <path key={`back-${angle}`} d={back} transform={`rotate(${angle + 22.5} 50 50)`} />
            ))}
          </g>
          {angles.map((angle) => (
            <path key={`front-${angle}`} d={front} transform={`rotate(${angle} 50 50)`} />
          ))}
        </g>
        <circle cx="50" cy="50" r="9" fill="black" />
      </mask>
      <rect width="100" height="100" fill="currentColor" mask={`url(#${maskId})`} />
    </svg>
  )
}
