// LO-02 — realistic load scenario.
//
// smoke.js proves the endpoints answer under nominal traffic; this models the
// real shape. The maths (from the ticket): 1M signups × ~3-5% active/day is a
// few tens of requests/second at peak. This ramps to that and a bit past it, on
// the read paths that dominate a browsing session — feed, search, discovery,
// leaderboards — so a regression in the DB pool or a hot query shows up as
// latency, not a mystery.
//
//   k6 run tests/load/realistic.js
//   BASE_URL=https://<mirror> k6 run tests/load/realistic.js
//
// To find the ceiling, raise the top stage's target and drop the thresholds:
// the point where http_req_failed climbs and p95 runs away is the plateau.

import http from "k6/http";
import { check, sleep } from "k6";
import { Rate } from "k6/metrics";

const BASE_URL = __ENV.BASE_URL || "http://localhost:3001";

const errors = new Rate("app_errors");

export const options = {
  scenarios: {
    ramp: {
      executor: "ramping-vus",
      startVUs: 0,
      stages: [
        { duration: "30s", target: 20 }, // warm up to nominal peak
        { duration: "1m", target: 40 }, // hold above peak
        { duration: "30s", target: 60 }, // push toward the ceiling
        { duration: "20s", target: 0 }, // ramp down
      ],
      gracefulStop: "10s",
    },
  },
  thresholds: {
    http_req_duration: ["p(95)<800"], // p95 under 800ms across the mix
    http_req_failed: ["rate<0.02"],
    app_errors: ["rate<0.02"],
  },
};

// A weighted browsing mix: mostly feed and discovery, some search, some
// leaderboards. Weights approximate what a real session hits.
const PATHS = [
  { path: "/api/feed/public", weight: 5 },
  { path: "/api/explore", weight: 3 },
  { path: "/api/opportunities", weight: 3 },
  { path: "/api/talents/search?q=rust", weight: 2 },
  { path: "/api/leaderboards", weight: 2 },
  { path: "/api/featured/code", weight: 2 },
  { path: "/api/health", weight: 1 },
];

const BAG = PATHS.flatMap((e) => Array(e.weight).fill(e.path));

function pick(iter) {
  // Deterministic per-VU/iteration spread without Math.random dependence on
  // wall clock — good enough to distribute the mix.
  return BAG[(iter + __VU) % BAG.length];
}

export default function () {
  const path = pick(__ITER);
  const res = http.get(`${BASE_URL}${path}`, { tags: { name: path.split("?")[0] } });
  const ok = check(res, {
    [`${path} not a server error`]: (r) => r.status < 500,
  });
  errors.add(!ok);
  sleep(Math.max(0.2, 1 - res.timings.duration / 1000));
}
