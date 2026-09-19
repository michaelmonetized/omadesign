import type { ConvexReactClient } from "convex/react";
import { api } from "../../convex/_generated/api";
import type { Id } from "../../convex/_generated/dataModel";
export async function upload(
  client: ConvexReactClient,
  projectId: Id<"cloudProjects">,
  file: File,
  kind: "source" | "asset" | "snapshot",
) {
  if (file.size > (kind === "snapshot" ? 20 : 100) * 1024 * 1024)
    throw new Error("File exceeds the upload limit.");
  let dimensions: {} | { width: number; height: number } = {};
  if (kind === "snapshot") {
    const image = await createImageBitmap(file);
    dimensions = { width: image.width, height: image.height };
    image.close();
  }
  const pending = await client.mutation(api.files.begin, { projectId, kind });
  const response = await fetch(pending.url, {
    method: "POST",
    headers: { "Content-Type": file.type || "application/octet-stream" },
    body: file,
  });
  if (!response.ok) throw new Error("Upload failed. Please retry.");
  const { storageId } = await response.json();
  return client.mutation(api.files.finish, {
    uploadId: pending.id,
    storageId,
    name: file.name,
    ...dimensions,
  });
}
export async function downloadBlob(id: Id<"cloudFiles">, token: string | null) {
  if (!token) throw new Error("Sign in again to download this file.");
  const response = await fetch(
    `${import.meta.env.VITE_CONVEX_SITE_URL}/file?id=${encodeURIComponent(id)}`,
    { headers: { Authorization: `Bearer ${token}` } },
  );
  if (!response.ok)
    throw new Error("This file is unavailable or your access has changed.");
  return response.blob();
}
export function saveBlob(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 10000);
}
export function message(error: unknown) {
  return error instanceof Error
    ? error.message.replace(/^\[CONVEX[^\]]*\]\s*/, "")
    : "Something went wrong. Please retry.";
}
