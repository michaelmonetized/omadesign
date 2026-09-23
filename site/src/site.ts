export const REPO = "https://github.com/michaelmonetized/omadesign";
export const DISCORD = "https://discord.gg/ejkZS2RBx";
export const SITE_ORIGIN = "https://omadesign.app";
export const PAGES_ORIGIN = "https://michaelmonetized.github.io/omadesign";
export const sitePath = (path = "") => {
  const clean = path.replace(/^\/+/, "");
  if (import.meta.env.VITE_PUBLIC_SITE_PREVIEW === "1" && /^(cloud|account|project|showcase|compete)(?:[/?#]|$)/.test(clean)) {
    return `${SITE_ORIGIN}/${clean}`;
  }
  return `${import.meta.env.BASE_URL}${clean}`;
};
export const media = (name: string) => sitePath(`media/studio/${name}`);
export const CURL = "curl -fsSL https://omadesign.app/install | sh";
