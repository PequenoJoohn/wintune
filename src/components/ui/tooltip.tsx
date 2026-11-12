import { Tooltip as TooltipPrimitive } from "radix-ui";
import type { ReactNode } from "react";

export const TooltipProvider = TooltipPrimitive.Provider;

interface TooltipProps {
  conteudo: ReactNode;
  children: ReactNode;
  lado?: "top" | "right" | "bottom" | "left";
}

/** Tooltip simples: `children` precisa ser um único elemento que aceite ref. */
export function Tooltip({ conteudo, children, lado = "top" }: TooltipProps) {
  return (
    <TooltipPrimitive.Root>
      <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content
          side={lado}
          align="start"
          sideOffset={6}
          collisionPadding={12}
          className="z-50 max-w-sm rounded-md border bg-popover px-3 py-2 text-xs leading-relaxed text-popover-foreground shadow-lg"
        >
          {conteudo}
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}
