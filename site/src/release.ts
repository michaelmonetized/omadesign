import { REPO } from "./site";

export const RELEASE_VERSION = "0.6.3-nightly.20261013.0001";
export const RELEASE_TAG = `v${RELEASE_VERSION}`;
export const RELEASE_URL = `${REPO}/releases/tag/${RELEASE_TAG}`;
export const NIGHTLY_VERSION = "0.6.7-nightly.20261016.0001";
export const NIGHTLY_TAG = `v${NIGHTLY_VERSION}`;
export const NIGHTLY_URL = `${REPO}/releases/tag/${NIGHTLY_TAG}`;

export const RELEASE_SOURCE_URL = `${REPO}/tree/${RELEASE_TAG}`;
export const SOURCE_BUILD = `git clone ${REPO}.git
cd omadesign
git checkout ${RELEASE_TAG}
cargo build --release --bin omadesign
./target/release/omadesign`;
