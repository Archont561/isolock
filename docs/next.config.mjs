import { createMDX } from "fumadocs-mdx/next";

const withMDX = createMDX();

const repository = (process.env.GITHUB_REPOSITORY ?? "isolock")
  .split("/")
  .pop();
const isProd = process.env.NODE_ENV === "production";
const basePath = isProd ? `/${repository}` : "";

/** @type {import('next').NextConfig} */
const config = {
  basePath,
  output: "export",
  reactStrictMode: true,
};

export default withMDX(config);
