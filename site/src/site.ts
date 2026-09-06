export const REPO = "https://github.com/michaelmonetized/omadesign";
export const sitePath = (path = "") =>
  `${import.meta.env.BASE_URL}${path.replace(/^\/+/, "")}`;
export const media = (name: string) => sitePath(`media/studio/${name}`);
export const CURL =
  "curl -fsSL https://raw.githubusercontent.com/michaelmonetized/omadesign/master/scripts/install-remote.sh | sh";
