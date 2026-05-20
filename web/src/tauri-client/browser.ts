import { invoke } from "@tauri-apps/api/core";

export interface UrlContextCard {
  title: string;
  summary: string;
  url: string;
  domain: string;
  charCount: number;
  fetchMethod: string;
}

export async function fetchUrlContext(url: string): Promise<UrlContextCard> {
  return invoke<UrlContextCard>("fetch_url_context", { url });
}
