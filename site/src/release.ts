import { REPO } from "./site";

export const RELEASE_VERSION = "0.0.5-alpha";
export const RELEASE_TAG = `v${RELEASE_VERSION}`;
export const RELEASE_URL = `${REPO}/releases/tag/${RELEASE_TAG}`;

export const RELEASE_SOURCE_URL = `${REPO}/tree/${RELEASE_TAG}`;
export const SOURCE_BUILD = `git clone ${REPO}.git
cd omadesign
git checkout ${RELEASE_TAG}
cargo build --release --bin omadesign
./target/release/omadesign`;
