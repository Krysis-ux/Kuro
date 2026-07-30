# The Kuro mark

The eight-petalled flower is an original SVG redraw, adapted from the flower
icon in [Rainette Music](https://github.com/Krysis-ux/Rainette-music)
(`web/assets/rainette-icon-256.png`) by the same author.

Differences from the source:

- Redrawn as vector paths rather than traced from the bitmap.
- Recoloured to pure monochrome; the original is sage green and cream.
- The centre disc is punched out with a mask so the mark works on any
  background rather than only on a dark rounded square.

It lives in two places, which must be kept in step:

| File | Used for |
| --- | --- |
| `src/components/Logo.tsx` | Everywhere in the interface. Petals are generated from one path plus rotations. |
| `public/favicon.svg` | Browser tab. Same geometry written out, with a `prefers-color-scheme` rule since a favicon cannot inherit `currentColor`. |

If the petal geometry changes, update both.
