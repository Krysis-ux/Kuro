# The Kuro mark

A rounded lowercase k whose upper arm is a dot: a 15-unit stem, a 15-unit leg
from the stem's middle to the bottom right, and a 26-unit disc where the upper
arm would be. Drawn on a 100-unit grid in pure monochrome so it works on any
background and stays legible at 16px.

It lives in four places, which must be kept in step:

| File | Used for |
| --- | --- |
| `src/components/Logo.tsx` | Everywhere in the browser interface. |
| `public/favicon.svg` | Browser tab, with a `prefers-color-scheme` rule since a favicon cannot inherit `currentColor`. |
| `macos/Sources/Kuro/Views/Theme.swift` (`KuroGlyph`) | Everywhere in the macOS app. |
| `packaging/macos/icon.svg` | The app icon: the mark in white on a charcoal tile. |

If the geometry changes, update all four.
