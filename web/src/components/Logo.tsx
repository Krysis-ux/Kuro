import { useId } from 'react'

/**
 * The Kuro mark: an eight-petalled flower with a punched-out centre.
 *
 * Adapted from the flower icon in the author's own Rainette Music project and
 * redrawn in pure monochrome for Kuro. Petals are generated rather than written
 * out so the two rings stay in step, and the centre is removed with a mask so
 * the mark sits correctly on any background.
 */
export function Logo({ size = 24, className }: { size?: number; className?: string }) {
  // Multiple logos can appear on one page, so the mask needs a unique id.
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
