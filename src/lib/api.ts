import { invoke } from "@tauri-apps/api/core";

export type StartType = "automatic" | "automatic_delayed" | "manual" | "disabled" | "unknown";

export type ServiceStatus =
  | "running"
  | "stopped"
  | "start_pending"
  | "stop_pending"
  | "paused"
  | "unknown";

export type Risco = "seguro" | "moderado" | "avancado";

export interface ServiceDto {
  name: string;
  display_name: string;
  status: ServiceStatus;
  start_type: StartType;
  categoria: string | null;
  risco: Risco | null;
  descricao: string | null;
  recomendado: StartType | null;
  protegido: boolean;
  restrito: boolean;
  motivo_protecao: string | null;
  original_start_type: StartType | null;
  alterado: boolean;
}

export interface ItemRelatorio {
  name: string;
  display_name: string;
  detalhe: string;
  aviso: string | null;
}

export interface ApplyReport {
  sucessos: ItemRelatorio[];
  falhas: ItemRelatorio[];
  ignorados: ItemRelatorio[];
  snapshot_id: string | null;
}

export type SnapshotKind = "original" | "automatico" | "manual";

export interface SnapshotInfo {
  id: string;
  tipo: SnapshotKind;
  criado_em: string;
  motivo: string;
  total_servicos: number;
}

export interface ItemPerfil {
  name: string;
  start_type: StartType;
}

export interface Perfil {
  id: string;
  nome: string;
  descricao: string;
  embutido: boolean;
  confirmacao_extra: boolean;
  itens: ItemPerfil[];
}

export type Situacao = "alterar" | "sem_mudanca" | "ausente" | "protegido";

export interface MudancaPrevista {
  name: string;
  display_name: string;
  de: StartType | null;
  para: StartType;
  situacao: Situacao;
  motivo: string | null;
  rodando: boolean;
  risco: Risco | null;
  categoria: string | null;
}

export interface SystemStats {
  ram_total: number;
  ram_usada: number;
  elevado: boolean;
}

export interface RestorePointResult {
  status: "criado" | "recente";
  mensagem: string;
}

/** Id fixo do snapshot criado na primeira execução. */
export const ID_ORIGINAL = "original";

export const api = {
  listServices: () => invoke<ServiceDto[]>("list_services"),
  setServiceEnabled: (name: string, enabled: boolean) =>
    invoke<string | null>("set_service_enabled", { name, enabled }),
  setStartType: (name: string, startType: StartType) =>
    invoke<string | null>("set_start_type", { name, startType }),
  listProfiles: () => invoke<Perfil[]>("list_profiles"),
  previewProfile: (profileId: string) =>
    invoke<MudancaPrevista[]>("preview_profile", { profileId }),
  applyProfile: (profileId: string) => invoke<ApplyReport>("apply_profile", { profileId }),
  saveCustomProfile: (nome: string) => invoke<Perfil>("save_custom_profile", { nome }),
  deleteProfile: (profileId: string) => invoke<null>("delete_profile", { profileId }),
  createSnapshot: () => invoke<SnapshotInfo>("create_snapshot"),
  listSnapshots: () => invoke<SnapshotInfo[]>("list_snapshots"),
  restoreSnapshot: (id: string) => invoke<ApplyReport>("restore_snapshot", { id }),
  deleteSnapshot: (id: string) => invoke<null>("delete_snapshot", { id }),
  createRestorePoint: () => invoke<RestorePointResult>("create_restore_point"),
  systemStats: () => invoke<SystemStats>("system_stats"),
};

/** O backend devolve erros como texto em PT-BR. */
export function mensagemDeErro(erro: unknown): string {
  if (typeof erro === "string") return erro;
  if (erro instanceof Error) return erro.message;
  return "Erro inesperado.";
}
