import { metricsResponse } from "./public-metrics";

/**
 * Answer the public download-count route.
 * Same payload as the homepage chips. Package downloads stay labeled separately from people.
 * @param request The incoming request. A `badge` query returns one Shields badge.
 * @returns The counts response.
 */
export function downloadStats(request?: Request) {
  return metricsResponse(request);
}
