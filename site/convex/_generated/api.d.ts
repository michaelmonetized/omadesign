/* eslint-disable */
/**
 * Generated `api` utility.
 *
 * THIS CODE IS AUTOMATICALLY GENERATED.
 *
 * To regenerate, run `npx convex dev`.
 * @module
 */

import type * as cloudAuth from "../cloudAuth.js";
import type * as cloudMaintenance from "../cloudMaintenance.js";
import type * as cloudSchema from "../cloudSchema.js";
import type * as crons from "../crons.js";
import type * as devices from "../devices.js";
import type * as files from "../files.js";
import type * as http from "../http.js";
import type * as mail from "../mail.js";
import type * as projects from "../projects.js";
import type * as review from "../review.js";
import type * as showcase from "../showcase.js";
import type * as telemetry from "../telemetry.js";
import type * as waitlist from "../waitlist.js";

import type {
  ApiFromModules,
  FilterApi,
  FunctionReference,
} from "convex/server";

declare const fullApi: ApiFromModules<{
  cloudAuth: typeof cloudAuth;
  cloudMaintenance: typeof cloudMaintenance;
  cloudSchema: typeof cloudSchema;
  crons: typeof crons;
  devices: typeof devices;
  files: typeof files;
  http: typeof http;
  mail: typeof mail;
  projects: typeof projects;
  review: typeof review;
  showcase: typeof showcase;
  telemetry: typeof telemetry;
  waitlist: typeof waitlist;
}>;

/**
 * A utility for referencing Convex functions in your app's public API.
 *
 * Usage:
 * ```js
 * const myFunctionReference = api.myModule.myFunction;
 * ```
 */
export declare const api: FilterApi<
  typeof fullApi,
  FunctionReference<any, "public">
>;

/**
 * A utility for referencing Convex functions in your app's internal API.
 *
 * Usage:
 * ```js
 * const myFunctionReference = internal.myModule.myFunction;
 * ```
 */
export declare const internal: FilterApi<
  typeof fullApi,
  FunctionReference<any, "internal">
>;

export declare const components: {
  resend: import("@convex-dev/resend/_generated/component.js").ComponentApi<"resend">;
};
