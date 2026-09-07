import { REPO } from "./site";

export const RELEASE_VERSION = "0.0.1-alpha";
export const RELEASE_TAG = `v${RELEASE_VERSION}`;
export const RELEASE_URL = `${REPO}/releases/tag/${RELEASE_TAG}`;

// Keep preview instructions and source links on the code documented by this site.
export const PREVIEW_COMMIT = "5bee4aae7432dc87ceaa905762b1b81037740ed9";
export const PREVIEW_URL = `${REPO}/tree/${PREVIEW_COMMIT}`;
export const PREVIEW_BUILD = `git clone ${REPO}.git
cd omadesign
git checkout ${PREVIEW_COMMIT}
cargo build --release --bin omadesign
./target/release/omadesign`;
