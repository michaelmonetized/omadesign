import { cronJobs } from "convex/server";
import { internal } from "./_generated/api";
const crons = cronJobs();
crons.interval(
  "Remove expired waitlist rate limits",
  { hours: 1 },
  internal.waitlist.cleanRateLimits,
);
crons.interval(
  "Expire cloud device requests and upload intents",
  { minutes: 10 },
  internal.cloudMaintenance.expire,
);
export default crons;
