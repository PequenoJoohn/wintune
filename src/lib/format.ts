import type { Risco, ServiceStatus, SnapshotKind, StartType } from "./api";

export const ROTULO_START_TYPE: Record<StartType, string> = {
  automatic: "Automático",
  automatic_delayed: "Automático (atrasado)",
  manual: "Manual",
  disabled: "Desativado",
  unknown: "Desconhecido",
};

/** Tipos de inicialização que o usuário pode escolher. */
export const START_TYPES_APLICAVEIS: StartType[] = [
  "automatic",
  "automatic_delayed",
  "manual",
  "disabled",
];

export const ROTULO_STATUS: Record<ServiceStatus, string> = {
  running: "Em execução",
  stopped: "Parado",
  start_pending: "Iniciando",
  stop_pending: "Parando",
  paused: "Pausado",
  unknown: "Desconhecido",
};

export const ROTULO_RISCO: Record<Risco, string> = {
  seguro: "Seguro",
  moderado: "Moderado",
  avancado: "Avançado",
};

export const ROTULO_SNAPSHOT: Record<SnapshotKind, string> = {
  original: "Original",
  automatico: "Automático",
  manual: "Manual",
};

export const CATEGORIAS = [
  "Telemetria",
  "Xbox/Jogos",
  "Impressão",
  "Indexação/Busca",
  "Rede remota",
  "Bluetooth",
  "Hyper-V",
  "Mapas/Localização",
  "Fax",
  "Diversos",
] as const;

export function estaAtivo(status: ServiceStatus): boolean {
  return status === "running" || status === "start_pending" || status === "paused";
}

export function formatarBytes(bytes: number): string {
  const gb = bytes / 1024 ** 3;
  return `${gb.toLocaleString("pt-BR", { minimumFractionDigits: 1, maximumFractionDigits: 1 })} GB`;
}

export function formatarData(iso: string): string {
  const data = new Date(iso);
  if (Number.isNaN(data.getTime())) return iso;
  return data.toLocaleString("pt-BR", {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function plural(quantidade: number, singular: string, pluralTexto: string): string {
  return `${quantidade} ${quantidade === 1 ? singular : pluralTexto}`;
}
