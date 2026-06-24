/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  // This app is self-contained under src/regent-web; pin the workspace root so
  // Turbopack doesn't latch onto the repo-root lockfile.
  turbopack: { root: import.meta.dirname },
};

export default nextConfig;
