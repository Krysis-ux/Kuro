import { useState, type CSSProperties, type ReactNode } from 'react'
import { hubAvatarUrls, modelBrand, providerBrand, type Brand } from '../lib/brands'

/**
 * The maker's mark beside a model or provider name.
 *
 * Every variant draws the same rounded tile, so a list mixing known brands,
 * Hub avatars and letters still lines up. `bare` drops the tile for places too
 * small to frame it, such as rows in the model picker.
 */

interface TileProps {
  size?: number
  bare?: boolean
  className?: string
}

export function ModelLogo({
  name,
  owner,
  ...tile
}: TileProps & { name: string; owner?: string | null }) {
  // A model's own name wins over its repository's owner, which for GGUF builds
  // is usually the quantizer rather than whoever trained it.
  const brand = modelBrand(name) ?? modelBrand(owner)
  if (brand) return <BrandTile brand={brand} {...tile} />
  if (owner) return <HubAvatar owner={owner} fallback={name} {...tile} />
  return <Monogram text={name} {...tile} />
}

export function ProviderLogo({ slug, label, ...tile }: TileProps & { slug: string; label: string }) {
  const brand = providerBrand(slug) ?? modelBrand(slug)
  return brand ? <BrandTile brand={brand} {...tile} /> : <Monogram text={label} {...tile} />
}

/** A Hub account's avatar, falling back to a letter when it has none or the Hub is unreachable. */
export function HubAvatar({ owner, fallback, ...tile }: TileProps & { owner: string; fallback?: string }) {
  const urls = hubAvatarUrls(owner)
  const [attempt, setAttempt] = useState(0)
  const src = urls[attempt]

  if (!src) return <Monogram text={fallback ?? owner} {...tile} />
  return (
    <Tile {...tile} variant="is-avatar" title={owner}>
      <img src={src} alt="" loading="lazy" onError={() => setAttempt((n) => n + 1)} />
    </Tile>
  )
}

function BrandTile({ brand, ...tile }: TileProps & { brand: Brand }) {
  return (
    <Tile {...tile} title={brand.label}>
      {brand.mono ? (
        // Masked rather than drawn as an image so a black mark becomes the text
        // colour, which is the only way it stays visible on the dark theme.
        <span
          className="logo-mono"
          style={{ maskImage: `url("${brand.src}")`, WebkitMaskImage: `url("${brand.src}")` }}
        />
      ) : (
        <img src={brand.src} alt="" />
      )}
    </Tile>
  )
}

function Monogram({ text, ...tile }: TileProps & { text: string }) {
  // The last path segment, so `unsloth/foo` reads as F rather than U.
  const word = text.split('/').pop() ?? text
  const letter = word.replace(/[^A-Za-z0-9]/g, '').charAt(0).toUpperCase() || '?'
  return (
    <Tile {...tile} variant="is-monogram" title={text}>
      {letter}
    </Tile>
  )
}

function Tile({
  size = 32,
  bare = false,
  className = '',
  variant = '',
  title,
  children,
}: TileProps & { variant?: string; title?: string; children: ReactNode }) {
  return (
    <span
      className={`logo-tile ${variant} ${bare ? 'is-bare' : ''} ${className}`}
      style={{ '--logo-size': `${size}px` } as CSSProperties}
      title={title}
      aria-hidden="true"
    >
      {children}
    </span>
  )
}
