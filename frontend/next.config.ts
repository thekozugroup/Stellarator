import type { NextConfig } from "next";

// Backend reachable from the frontend container (docker network) OR a host override.
const BACKEND = process.env.BACKEND_INTERNAL_URL || "http://backend:8000";

const config: NextConfig = {
  reactStrictMode: true,
  experimental: { typedRoutes: false },
  async rewrites() {
    // Proxy backend API + websockets through Next.js so the browser only ever
    // talks to the same origin. Fixes "Failed to fetch" when accessing the UI
    // from a host other than localhost (LAN, Tailscale) where the browser's
    // localhost:8000 resolves to the wrong machine.
    return [
      { source: "/v1/:path*", destination: `${BACKEND}/v1/:path*` },
      { source: "/healthz",   destination: `${BACKEND}/healthz` },
      { source: "/metrics",   destination: `${BACKEND}/metrics` },
      { source: "/ws/:path*", destination: `${BACKEND}/ws/:path*` },
    ];
  },
};

export default config;
