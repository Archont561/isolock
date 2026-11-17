import Link from "fumadocs-core/link";
import { buttonVariants } from "fumadocs-ui/components/ui/button";
import { HomeLayout } from "fumadocs-ui/layouts/home";

export default function HomePage() {
  return (
    <HomeLayout>
      <main className="mx-auto flex min-h-[calc(100vh-4rem)] max-w-3xl flex-col justify-center gap-6 px-6 py-16">
        <p className="text-sm font-medium uppercase tracking-wide opacity-70">
          isolock v0.1.0 · Hello World
        </p>
        <h1 className="text-4xl font-bold tracking-tight">Isolock Documentation</h1>
        <p className="text-lg opacity-80">
          Dedicated, deterministic dependency airlock and local registry proxy manager for Conda,
          PyPI, npm, and Cargo ecosystems.
        </p>
        <div className="flex flex-wrap gap-3">
          <Link href="/docs" className={`${buttonVariants({ variant: "primary" })} px-4 py-2`}>
            Read the Docs
          </Link>
          <Link
            href="/docs/getting-started"
            className={`${buttonVariants({ variant: "outline" })} px-4 py-2`}
          >
            Getting Started
          </Link>
          <Link
            href="/docs/architecture"
            className={`${buttonVariants({ variant: "outline" })} px-4 py-2`}
          >
            Architecture
          </Link>
        </div>
      </main>
    </HomeLayout>
  );
}
