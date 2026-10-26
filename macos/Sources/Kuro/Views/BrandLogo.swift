import AppKit
import SwiftUI
import KuroKit

/// The maker's mark beside a model or provider name.
///
/// Every variant draws the same rounded tile, so a list mixing brand marks, Hub
/// avatars and letters still lines up. `bare` drops the tile for rows too small
/// to frame it, such as the model picker's.
struct ModelLogo: View {
    let name: String
    var owner: String?
    var size: CGFloat = 28
    var bare = false

    var body: some View {
        // The model's own name wins over its repository's owner, which for GGUF
        // builds is usually the quantizer rather than whoever trained it.
        if let brand = Brands.model(name) ?? Brands.model(owner) {
            BrandTile(brand: brand, size: size, bare: bare)
        } else if let owner, !owner.isEmpty {
            HubAvatar(owner: owner, fallback: name, size: size, bare: bare)
        } else {
            Monogram(text: name, size: size, bare: bare)
        }
    }
}

struct ProviderLogo: View {
    let slug: String
    let label: String
    var size: CGFloat = 28
    var bare = false

    var body: some View {
        if let brand = Brands.provider(slug) ?? Brands.model(slug) {
            BrandTile(brand: brand, size: size, bare: bare)
        } else {
            Monogram(text: label, size: size, bare: bare)
        }
    }
}

/// A Hub account's avatar, falling back to a letter when it has none or the Hub is unreachable.
struct HubAvatar: View {
    let owner: String
    var fallback: String?
    var size: CGFloat = 28
    var bare = false
    @State private var attempt = 0

    var body: some View {
        let urls = Brands.avatarURLs(owner: owner)
        if attempt < urls.count {
            AsyncImage(url: urls[attempt]) { phase in
                switch phase {
                case .success(let image):
                    image.resizable().scaledToFill()
                case .failure:
                    Color.clear.onAppear { attempt += 1 }
                default:
                    Color.primary.opacity(0.05)
                }
            }
            .frame(width: size, height: size)
            .clipShape(RoundedRectangle(cornerRadius: bare ? size / 4 : size * 0.28, style: .continuous))
            .overlay(tileBorder(size: size, bare: bare))
            .help(owner)
        } else {
            Monogram(text: fallback ?? owner, size: size, bare: bare)
        }
    }
}

private struct BrandTile: View {
    let brand: Brand
    let size: CGFloat
    let bare: Bool

    var body: some View {
        Tile(size: size, bare: bare) {
            if let image = LogoFiles.image(brand) {
                Image(nsImage: image)
                    .resizable()
                    // A black mark becomes the text colour, the only way it stays visible in dark mode.
                    .renderingMode(brand.mono ? .template : .original)
                    .interpolation(.high)
                    .scaledToFit()
                    .padding(bare ? 0 : size * 0.19)
            } else {
                Monogram(text: brand.label, size: size, bare: true)
            }
        }
        .help(brand.label)
    }
}

private struct Monogram: View {
    let text: String
    let size: CGFloat
    let bare: Bool

    var body: some View {
        // The last path segment, so `unsloth/foo` reads as F rather than U.
        let word = text.split(separator: "/").last.map(String.init) ?? text
        let letter = word.first(where: { $0.isLetter || $0.isNumber }).map { String($0).uppercased() } ?? "?"
        Tile(size: size, bare: bare) {
            Text(letter)
                .font(.system(size: size * 0.42, weight: .semibold, design: .rounded))
                .foregroundStyle(.secondary)
        }
        .help(text)
    }
}

private struct Tile<Content: View>: View {
    let size: CGFloat
    let bare: Bool
    @ViewBuilder let content: Content

    var body: some View {
        content
            .frame(width: size, height: size)
            .background {
                if !bare {
                    RoundedRectangle(cornerRadius: size * 0.28, style: .continuous)
                        .fill(LinearGradient(colors: [Color.primary.opacity(0.07), Color.primary.opacity(0.03)], startPoint: .top, endPoint: .bottom))
                }
            }
            .overlay(tileBorder(size: size, bare: bare))
            .accessibilityHidden(true)
    }
}

@ViewBuilder
private func tileBorder(size: CGFloat, bare: Bool) -> some View {
    if !bare {
        RoundedRectangle(cornerRadius: size * 0.28, style: .continuous)
            .strokeBorder(Color.primary.opacity(0.1))
    }
}

/// The logo SVGs: inside the app bundle, or the web interface's own copy when
/// running from a checkout, so both interfaces draw from one set of files.
@MainActor
enum LogoFiles {
    private static let directory: URL? = {
        let fileManager = FileManager.default
        if let bundled = Bundle.main.resourceURL?.appendingPathComponent("logos"), fileManager.fileExists(atPath: bundled.path) {
            return bundled
        }
        var directory = Bundle.main.executableURL?.deletingLastPathComponent()
        while let current = directory, current.path != "/" {
            let candidate = current.appendingPathComponent("web/src/assets/logos")
            if fileManager.fileExists(atPath: candidate.path) { return candidate }
            directory = current.deletingLastPathComponent()
        }
        return nil
    }()

    private static var cache: [String: NSImage] = [:]

    static func image(_ brand: Brand) -> NSImage? {
        if let cached = cache[brand.file] { return cached }
        guard let url = directory?.appendingPathComponent("\(brand.file).svg"),
              let image = NSImage(contentsOf: url) else { return nil }
        // The SVGs declare a 1em size; give them a real one so they are drawn as vectors at tile size.
        image.size = NSSize(width: 128, height: 128)
        cache[brand.file] = image
        return image
    }
}
