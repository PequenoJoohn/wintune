import { Gauge, Loader2, LogOut, RefreshCw, RotateCcw, ShieldAlert, Zap } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Tooltip } from "@/components/ui/tooltip";
import {
  api,
  mensagemDeErro,
  type AlteracaoEfeito,
  type ApplyReport,
  type EfeitoDto,
  type Impacto,
  type ListaEfeitos,
} from "@/lib/api";
import { plural } from "@/lib/format";
import { cn } from "@/lib/utils";

interface EffectsProps {
  mostrarRelatorio: (titulo: string, relatorio: ApplyReport) => void;
}

type PresetId = "desempenho" | "tudo" | "original";

interface Preset {
  id: PresetId;
  titulo: string;
  descricao: string;
  /** Estado desejado de cada efeito; `null` deixa como está. */
  alvo: (efeito: EfeitoDto) => boolean | null;
  perigoso?: boolean;
}

const PRESETS: Preset[] = [
  {
    id: "desempenho",
    titulo: "Melhor desempenho",
    descricao:
      "Desliga animações, sombras e transparência, mas mantém a suavização de fontes, as miniaturas e o conteúdo ao arrastar janelas.",
    alvo: (e) => e.manter_ligado,
  },
  {
    id: "tudo",
    titulo: "Desligar tudo",
    descricao:
      "Igual a \"Ajustar para obter melhor desempenho\" do Windows: desliga até a suavização de fontes, deixando o texto serrilhado.",
    alvo: () => false,
    perigoso: true,
  },
  {
    id: "original",
    titulo: "Restaurar original",
    descricao: "Volta cada efeito ao estado em que estava na primeira vez que o WinTune foi aberto.",
    alvo: (e) => e.original,
  },
];

const ROTULO_IMPACTO: Record<Impacto, string> = {
  alto: "Ganho alto",
  medio: "Ganho médio",
  baixo: "Ganho baixo",
};

const VARIANTE_IMPACTO = { alto: "success", medio: "primary", baixo: "outline" } as const;

function mudancasDoPreset(preset: Preset, efeitos: EfeitoDto[]): (AlteracaoEfeito & { nome: string })[] {
  return efeitos.flatMap((e) => {
    const alvo = preset.alvo(e);
    if (alvo === null || e.ligado === null || e.ligado === alvo) return [];
    return [{ id: e.id, ligado: alvo, nome: e.nome }];
  });
}

export function Effects({ mostrarRelatorio }: EffectsProps) {
  const [lista, setLista] = useState<ListaEfeitos | null>(null);
  const [carregando, setCarregando] = useState(true);
  const [ocupados, setOcupados] = useState<ReadonlySet<string>>(new Set());
  const [preset, setPreset] = useState<Preset | null>(null);
  const [aplicando, setAplicando] = useState(false);
  const [aConfirmar, setAConfirmar] = useState<EfeitoDto | null>(null);

  const carregar = useCallback(async () => {
    setCarregando(true);
    try {
      setLista(await api.listEffects());
    } catch (erro) {
      toast.error("Não foi possível ler os efeitos visuais", { description: mensagemDeErro(erro) });
    } finally {
      setCarregando(false);
    }
  }, []);

  useEffect(() => {
    let ativo = true;
    api
      .listEffects()
      .then((dados) => ativo && setLista(dados))
      .catch((erro: unknown) =>
        toast.error("Não foi possível ler os efeitos visuais", {
          description: mensagemDeErro(erro),
        }),
      )
      .finally(() => ativo && setCarregando(false));
    return () => {
      ativo = false;
    };
  }, []);

  const efeitos = useMemo(() => lista?.efeitos ?? [], [lista]);
  const bloqueado = lista?.bloqueio != null;
  const ligados = efeitos.filter((e) => e.ligado === true).length;
  const alterados = efeitos.filter((e) => e.alterado).length;
  const mudancas = useMemo(
    () => (preset ? mudancasDoPreset(preset, efeitos) : []),
    [preset, efeitos],
  );

  async function alternar(efeito: EfeitoDto, ligar: boolean) {
    setOcupados((atual) => new Set(atual).add(efeito.id));
    try {
      const aviso = await api.setEffect(efeito.id, ligar);
      const descricao = ligar ? "Ligado." : "Desligado.";
      if (aviso) {
        toast.info(efeito.nome, { description: `${descricao} ${aviso}`, duration: 9000 });
      } else {
        toast.success(efeito.nome, { description: descricao });
      }
    } catch (erro) {
      toast.error(`Não foi possível alterar "${efeito.nome}"`, {
        description: mensagemDeErro(erro),
        duration: 9000,
      });
    } finally {
      setOcupados((atual) => {
        const novo = new Set(atual);
        novo.delete(efeito.id);
        return novo;
      });
      await carregar();
    }
  }

  function pedirAlternar(efeito: EfeitoDto, ligar: boolean) {
    if (!ligar && efeito.manter_ligado) {
      setAConfirmar(efeito);
    } else {
      void alternar(efeito, ligar);
    }
  }

  async function aplicarPreset(escolhido: Preset) {
    setAplicando(true);
    try {
      const relatorio = await api.applyEffects(
        mudancas.map(({ id, ligado }) => ({ id, ligado })),
        `efeitos visuais: ${escolhido.titulo}`,
      );
      mostrarRelatorio(`Efeitos visuais: ${escolhido.titulo}`, relatorio);
    } catch (erro) {
      toast.error("Não foi possível aplicar", { description: mensagemDeErro(erro), duration: 12000 });
    } finally {
      setPreset(null);
      setAplicando(false);
      await carregar();
    }
  }

  const exigeLogoff = mudancas.some((m) => efeitos.find((e) => e.id === m.id)?.requer_logoff);

  return (
    <div className="flex h-full flex-col gap-4">
      <header className="flex items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold">Efeitos visuais</h1>
          <p className="text-sm text-muted-foreground">
            Animações, sombras e transparência do Windows. Desligá-los alivia a placa de vídeo e
            deixa a interface mais rápida em PCs modestos. Valem só para a sua conta de usuário.
          </p>
        </div>
        <Button variant="outline" onClick={() => void carregar()} disabled={carregando}>
          <RefreshCw className={cn(carregando && "animate-spin")} />
          Atualizar
        </Button>
      </header>

      {bloqueado && (
        <div className="flex items-start gap-3 rounded-xl border border-warning/40 bg-warning/10 p-4 text-sm">
          <ShieldAlert className="mt-0.5 size-5 shrink-0 text-warning" />
          <div>
            <p className="font-medium text-warning">Efeitos visuais bloqueados</p>
            <p className="text-muted-foreground">{lista?.bloqueio}</p>
          </div>
        </div>
      )}

      <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
        {PRESETS.map((p) => {
          const total = mudancasDoPreset(p, efeitos).length;
          const icone =
            p.id === "original" ? (
              <RotateCcw className="size-4" />
            ) : p.id === "tudo" ? (
              <Zap className="size-4" />
            ) : (
              <Gauge className="size-4" />
            );
          return (
            <Card key={p.id} className="flex flex-col gap-3 p-4">
              <div className="flex items-center gap-2 font-medium">
                {icone}
                {p.titulo}
                {p.id === "desempenho" && <Badge variant="success">Recomendado</Badge>}
              </div>
              <p className="flex-1 text-xs leading-relaxed text-muted-foreground">{p.descricao}</p>
              <Button
                variant={p.id === "desempenho" ? "default" : "outline"}
                size="sm"
                disabled={bloqueado || carregando || total === 0}
                onClick={() => setPreset(p)}
              >
                {total === 0 ? "Nada a alterar" : `Aplicar (${plural(total, "mudança", "mudanças")})`}
              </Button>
            </Card>
          );
        })}
      </div>

      <div className="flex items-center gap-3 text-sm text-muted-foreground">
        <span>
          {ligados} de {efeitos.length} ligados
        </span>
        {alterados > 0 && <Badge variant="primary">{plural(alterados, "alterado", "alterados")}</Badge>}
      </div>

      <Card className="min-h-0 flex-1 overflow-y-auto">
        {carregando && !lista ? (
          <div className="flex h-40 items-center justify-center text-muted-foreground">
            <Loader2 className="animate-spin" />
          </div>
        ) : (
          <ul className="divide-y">
            {efeitos.map((efeito) => (
              <li
                key={efeito.id}
                className="flex items-start gap-4 px-4 py-3 transition-colors hover:bg-accent/40"
              >
                <div className="w-11 pt-0.5">
                  {ocupados.has(efeito.id) ? (
                    <Loader2 className="ml-2 size-4 animate-spin text-muted-foreground" />
                  ) : (
                    <Switch
                      checked={efeito.ligado === true}
                      disabled={bloqueado || efeito.ligado === null}
                      onCheckedChange={(ligar) => pedirAlternar(efeito, ligar)}
                      aria-label={`${efeito.ligado ? "Desligar" : "Ligar"} ${efeito.nome}`}
                    />
                  )}
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-medium">{efeito.nome}</span>
                    {efeito.alterado && <Badge variant="primary">Alterado</Badge>}
                    {efeito.manter_ligado && <Badge variant="warning">Recomendado manter</Badge>}
                    {efeito.requer_logoff && (
                      <Tooltip conteudo="Pode ser preciso sair e entrar novamente na conta do Windows para ver a mudança.">
                        <span className="flex items-center gap-1 text-xs text-muted-foreground">
                          <LogOut className="size-3.5" />
                          Requer sair e entrar
                        </span>
                      </Tooltip>
                    )}
                    {efeito.ligado === null && !bloqueado && (
                      <span className="text-xs text-danger">Não foi possível ler</span>
                    )}
                  </div>
                  <p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">
                    {efeito.descricao}
                  </p>
                </div>
                <Badge variant={VARIANTE_IMPACTO[efeito.impacto]} className="shrink-0">
                  {ROTULO_IMPACTO[efeito.impacto]}
                </Badge>
              </li>
            ))}
          </ul>
        )}
      </Card>

      <ConfirmDialog
        aberto={preset !== null}
        ocupado={aplicando}
        perigoso={preset?.perigoso}
        titulo={preset ? `${preset.titulo}?` : ""}
        rotuloConfirmar="Aplicar"
        className="max-w-lg"
        descricao={
          <p>
            {plural(mudancas.length, "efeito será alterado", "efeitos serão alterados")}. Um backup
            automático é criado antes, e tudo pode ser desfeito em Backups ou em "Restaurar
            original".
            {exigeLogoff && " Algumas mudanças só aparecem depois de sair e entrar na conta."}
          </p>
        }
        onCancelar={() => setPreset(null)}
        onConfirmar={() => preset && void aplicarPreset(preset)}
      >
        <ul className="max-h-64 overflow-y-auto rounded-md border text-sm">
          {mudancas.map((m) => (
            <li key={m.id} className="flex items-center justify-between gap-3 border-b px-3 py-2 last:border-b-0">
              <span className="min-w-0 truncate">{m.nome}</span>
              <Badge variant={m.ligado ? "success" : "outline"} className="shrink-0">
                {m.ligado ? "Ligar" : "Desligar"}
              </Badge>
            </li>
          ))}
        </ul>
      </ConfirmDialog>

      <ConfirmDialog
        aberto={aConfirmar !== null}
        titulo={aConfirmar ? `Desligar "${aConfirmar.nome}"?` : ""}
        rotuloConfirmar="Desligar mesmo assim"
        perigoso
        descricao={
          <p>
            {aConfirmar?.descricao} O ganho de desempenho é pequeno e o Windows fica mais difícil
            de usar. Você pode ligar de novo a qualquer momento.
          </p>
        }
        onCancelar={() => setAConfirmar(null)}
        onConfirmar={() => {
          if (aConfirmar) void alternar(aConfirmar, false);
          setAConfirmar(null);
        }}
      />
    </div>
  );
}
