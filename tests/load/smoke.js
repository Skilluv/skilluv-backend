// k6 smoke + light load test — asserts the API stays healthy under nominal
// traffic. Run locally:
//   k6 run tests/load/smoke.js
// Or targeting a real deployment:
//   BASE_URL=https://api.skill-uv.com k6 run tests/load/smoke.js
//
// This is a smoke test, not a stress test — it verifies the endpoints
// respond correctly under ~10 concurrent users for 30s. The real
// stress/soak scenarios belong in tests/load/stress.js (add when needed).

import http from "k6/http";
import { check, sleep } from "k6";

const BASE_URL = __ENV.BASE_URL || "http://localhost:3001";

export const options = {
  scenarios: {
    smoke: {
      executor: "constant-vus",
      vus: 10,           // 10 concurrent virtual users
      duration: "30s",   // for 30s
      gracefulStop: "5s",
    },
  },
  thresholds: {
    // p95 latency under 500ms — realistic for a Rust axum backend on a
    // small VPS. Bump if the target is a bigger box.
    http_req_duration: ["p(95)<500"],
    // No more than 1% failed requests.
    http_req_failed: ["rate<0.01"],
    // Every check must pass.
    checks: ["rate>0.99"],
  },
};

export default function () {
  // Cheap read paths — no auth required. Enough to prove the routing,
  // middleware and DB pool are functional under load.
  const endpoints = [
    "/api/health",
    "/api/pricing",
  ];

  for (const path of endpoints) {
    const res = http.get(`${BASE_URL}${path}`);
    check(res, {
      [`${path} status is 200`]: (r) => r.status === 200,
      [`${path} responded under 500ms`]: (r) => r.timings.duration < 500,
    });
  }

  sleep(1); // 1s between iterations per VU = ~10 req/s total
}
