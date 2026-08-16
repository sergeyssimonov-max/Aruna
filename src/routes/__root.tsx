import { Outlet, createRootRoute, HeadContent, Scripts } from "@tanstack/react-router";
import appCss from "../styles.css?url";
// The same import `load-inventory` uses, so the preload below cannot point at a
// name the build no longer emits — which is exactly what it did once the file
// gained a content hash.
import inventoryGzip from "@/data/inventory.bin.gz?url";

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: "utf-8" },
      { name: "viewport", content: "width=device-width, initial-scale=1" },
      { title: "Thesaurus Linguarum Hethaeorum Digitalis" },
      {
        name: "description",
        content: "TLHdig inventory — Hittite cuneiform manuscripts (Zenodo Beta 0.3)",
      },
    ],
    links: [
      { rel: "stylesheet", href: appCss },
      { rel: "icon", href: "/favicon.svg", type: "image/svg+xml" },
      {
        rel: "preload",
        href: inventoryGzip,
        as: "fetch",
        type: "application/octet-stream",
        crossOrigin: "anonymous" as const,
      },
    ],
  }),
  component: () => (
    <html lang="en">
      <head>
        <HeadContent />
      </head>
      <body className="font-sans antialiased">
        <Outlet />
        <Scripts />
      </body>
    </html>
  ),
});
