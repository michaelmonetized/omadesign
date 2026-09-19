import { cronJobs } from "convex/server";
import { internal } from "./_generated/api";
const crons = cronJobs();
crons.interval("Remove expired waitlist rate limits", { hours: 1 }, internal.waitlist.cleanRateLimits);
export default crons;
