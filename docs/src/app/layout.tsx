import { RootProvider } from "fumadocs-ui/provider/next";
import type { Metadata } from "next";
import type { ReactNode } from "react";
import "./global.css";

export const metadata: Metadata = {
  title: {
    default: "Isolock Documentation",
    template: "%s | Isolock Documentation",
  },
  description:
    "Hermetic multi-ecosystem dependency airlock and local registry proxy manager.",
};

export default function RootLayout({ children }: { readonly children: ReactNode }) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body>
        <RootProvider>{children}</RootProvider>
      </body>
    </html>
  );
}
