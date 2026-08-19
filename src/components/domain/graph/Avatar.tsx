import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";
import { avatarUrl } from "@/lib/avatarSource";
import { botIdentity } from "@/lib/brandLogos";

interface AvatarProps {
  initials: string;
  color: string;
  email?: string;
  size?: "sm" | "md";
  /**
   * Bump to re-run the lookup. The browser caches the image itself, so this
   * also rides along on the URL to defeat that second cache - without it a
   * refresh would re-probe the network and still paint the old picture.
   */
  reloadKey?: number;
}

/**
 * Whether a commit was authored by Dependabot.
 *
 * Kept as its own export because callers ask this specific question; the
 * general "which bot is this?" lookup lives in `brandLogos`.
 */
export function isDependabot(email: string): boolean {
  return /^(?:\d+\+)?dependabot(?:\[bot\])?@users\.noreply\.github\.com$/i.test(
    email.trim(),
  );
}

export function Avatar({
  initials,
  color,
  email,
  size = "sm",
  reloadKey = 0,
}: AvatarProps) {
  const px = size === "sm" ? 19 : 26;
  // A bot commit shows the tool's own mark: two-letter initials ("DE") read as
  // a stranger on the team, and these accounts have no Gravatar to find.
  const bot = email ? botIdentity(email) : null;
  const [src, setSrc] = useState<string | null>(null);

  // `botIdentity` returns a fresh object each render, so the effect depends on
  // whether there is a bot rather than on the object itself - otherwise the
  // lookup would re-run on every render.
  const isBot = !!bot;

  useEffect(() => {
    if (!email || isBot) return;
    let cancelled = false;
    void avatarUrl(email, px * 2).then((url) => {
      if (cancelled) return;
      setSrc(url && reloadKey ? `${url}&_r=${reloadKey}` : url);
    });
    return () => {
      cancelled = true;
    };
  }, [email, px, isBot, reloadKey]);

  if (bot) {
    return (
      <span
        title={bot.name}
        className={cn(
          "flex flex-none items-center justify-center overflow-hidden rounded-full text-foreground",
          size === "sm" ? "size-[19px]" : "size-[26px]",
        )}
        style={{ background: color + "2b" }}
      >
        <img
          src={bot.logo}
          alt=""
          aria-hidden
          width={size === "sm" ? 12 : 16}
          height={size === "sm" ? 12 : 16}
        />
      </span>
    );
  }

  if (src) {
    return (
      <img
        src={src}
        alt=""
        className={cn(
          "flex-none rounded-full",
          size === "sm" ? "size-[19px]" : "size-[26px]",
        )}
        style={{ border: `1px solid ${color}55` }}
      />
    );
  }

  return (
    <span
      className={cn(
        "flex flex-none items-center justify-center rounded-full font-bold",
        size === "sm" ? "size-[19px] text-2xs" : "size-[26px] text-2xs",
      )}
      style={{
        background: color + "2b",
        color,
        border: `1px solid ${color}55`,
      }}
    >
      {initials}
    </span>
  );
}
