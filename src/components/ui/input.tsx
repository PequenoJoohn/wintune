import type { InputHTMLAttributes, SelectHTMLAttributes } from "react";
import { cn } from "@/lib/utils";

const base =
  "h-9 rounded-md border border-input bg-transparent px-3 text-sm outline-none transition-colors placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-50";

export function Input({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return <input className={cn(base, "w-full select-text", className)} {...props} />;
}

/** Select nativo estilizado (leve o bastante para uma linha por serviço). */
export function Select({ className, ...props }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select
      className={cn(base, "cursor-pointer pr-2 [&>option]:bg-popover [&>option]:text-popover-foreground", className)}
      {...props}
    />
  );
}
