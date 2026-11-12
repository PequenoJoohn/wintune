import { ArrowRight, Gamepad2, Feather, Loader2, Save, Trash2, UserCog, Zap } from "lucide-react";
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { RiskBadge } from "@/components/RiskBadge";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  api,
  mensagemDeErro,
  type ApplyReport,
  type MudancaPrevista,
  type Perfil,
  type ServiceDto,
} from "@/lib/api";
import { ROTULO_START_TYPE, plural } from "@/lib/format";

interface ProfilesProps {
  servicos: ServiceDto[];
  recarregarServicos: () => Promise<void>;
  mostrarRelatorio: (titulo: string, relatorio: ApplyReport) => void;
}

const ICONES: Record<string, ReactNode> = {
  leve: <Feather className="size-5 text-success" />,
  gamer: <Gamepad2 className="size-5 text-primary" />,
  maximo: <Zap className="size-5 text-warning" />,
};

function Checkbox({
  marcado,
  onChange,
  disabled,
  children,
}: {
  marcado: boolean;
  onChange: (valor: boolean) => void;
  disabled?: boolean;
  children: ReactNode;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-2 text-sm">
      <input
        type="checkbox"
        className="mt-0.5 size-4 accent-(--primary)"
        checked={marcado}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span>{children}</span>
    </label>
  );
}

/** Prévia (diff) do perfil: o que muda, e o que será ignorado e por quê. */
function Previa({ previa }: { previa: MudancaPrevista[] }) {
  const alterar = previa.filter((m) => m.situacao === "alterar");
  const ignorados = previa.filter((m) => m.situacao !== "alterar");

  return (
    <div className="flex min-h-0 flex-col gap-2 overflow-y-auto pr-1">
      {alterar.length === 0 ? (
        <p className="rounded-lg border bg-card/50 px-3 py-4 text-center text-sm text-muted-foreground">
          Nada a alterar: este perfil já está totalmente aplicado neste computador.
        </p>
      ) : (
        <div className="rounded-lg border bg-card/50">
          <div className="border-b px-3 py-2 text-sm font-medium">
            Serão alterados ({alterar.length})
          </div>
          <ul className="divide-y text-sm">
            {alterar.map((m) => (
              <li key={m.name} className="flex items-center gap-3 px-3 py-2">
                <div className="min-w-0 flex-1">
                  <div className="truncate font-medium">{m.display_name}</div>
                  <div className="truncate font-mono text-xs text-muted-foreground">
                    {m.name}
                    {m.rodando && " · em execução, será parado"}
                  </div>
                </div>
                <RiskBadge risco={m.risco} />
                <span className="flex w-64 shrink-0 items-center justify-end gap-1.5 text-xs">
                  <span className="text-muted-foreground">
                    {m.de ? ROTULO_START_TYPE[m.de] : "—"}
                  </span>
                  <ArrowRight className="size-3 text-muted-foreground" />
                  <span className="font-medium text-danger">{ROTULO_START_TYPE[m.para]}</span>
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {ignorados.length > 0 && (
        <details className="rounded-lg border bg-card/50">
          <summary className="cursor-pointer px-3 py-2 text-sm font-medium text-muted-foreground">
            Não serão alterados ({ignorados.length})
          </summary>
          <ul className="divide-y border-t text-xs">
            {ignorados.map((m) => (
              <li key={m.name} className="flex justify-between gap-3 px-3 py-1.5">
                <span className="truncate">{m.display_name}</span>
                <span className="shrink-0 text-muted-foreground">{m.motivo}</span>
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}

export function Profiles({ servicos, recarregarServicos, mostrarRelatorio }: ProfilesProps) {
  const [perfis, setPerfis] = useState<Perfil[]>([]);
  const [carregando, setCarregando] = useState(true);
  const [selecionado, setSelecionado] = useState<Perfil | null>(null);
  const [previa, setPrevia] = useState<MudancaPrevista[] | null>(null);
  const [criarPonto, setCriarPonto] = useState(false);
  const [ciente, setCiente] = useState(false);
  const [aplicando, setAplicando] = useState(false);
  const [nomeNovo, setNomeNovo] = useState("");
  const [salvando, setSalvando] = useState(false);
  const [aExcluir, setAExcluir] = useState<Perfil | null>(null);

  const alterados = useMemo(() => servicos.filter((s) => s.alterado).length, [servicos]);

  const carregar = useCallback(async () => {
    try {
      setPerfis(await api.listProfiles());
    } catch (erro) {
      toast.error("Não foi possível carregar os perfis", { description: mensagemDeErro(erro) });
    } finally {
      setCarregando(false);
    }
  }, []);

  useEffect(() => {
    let ativo = true;
    api
      .listProfiles()
      .then((lista) => ativo && setPerfis(lista))
      .catch((erro: unknown) =>
        toast.error("Não foi possível carregar os perfis", { description: mensagemDeErro(erro) }),
      )
      .finally(() => ativo && setCarregando(false));
    return () => {
      ativo = false;
    };
  }, []);

  async function abrirPrevia(perfil: Perfil) {
    setSelecionado(perfil);
    setPrevia(null);
    setCriarPonto(false);
    setCiente(false);
    try {
      setPrevia(await api.previewProfile(perfil.id));
    } catch (erro) {
      toast.error("Não foi possível gerar a prévia", { description: mensagemDeErro(erro) });
      setSelecionado(null);
    }
  }

  async function aplicar(perfil: Perfil) {
    setAplicando(true);
    try {
      if (criarPonto) {
        // Se o usuário pediu o ponto de restauração e ele falhar, nada é aplicado.
        const ponto = await api.createRestorePoint();
        toast.info("Ponto de restauração do Windows", { description: ponto.mensagem });
      }
      const relatorio = await api.applyProfile(perfil.id);
      setSelecionado(null);
      mostrarRelatorio(`Perfil ${perfil.nome} aplicado`, relatorio);
    } catch (erro) {
      toast.error(`O perfil ${perfil.nome} não foi aplicado`, {
        description: mensagemDeErro(erro),
        duration: 12000,
      });
    } finally {
      setAplicando(false);
      await recarregarServicos();
    }
  }

  async function salvarPersonalizado() {
    setSalvando(true);
    try {
      const perfil = await api.saveCustomProfile(nomeNovo);
      toast.success(`Perfil "${perfil.nome}" salvo`, {
        description: plural(perfil.itens.length, "serviço incluído.", "serviços incluídos."),
      });
      setNomeNovo("");
      await carregar();
    } catch (erro) {
      toast.error("Não foi possível salvar o perfil", { description: mensagemDeErro(erro) });
    } finally {
      setSalvando(false);
    }
  }

  async function excluir(perfil: Perfil) {
    try {
      await api.deleteProfile(perfil.id);
      toast.success(`Perfil "${perfil.nome}" excluído`);
    } catch (erro) {
      toast.error("Não foi possível excluir o perfil", { description: mensagemDeErro(erro) });
    } finally {
      setAExcluir(null);
      await carregar();
    }
  }

  const mudancas = previa?.filter((m) => m.situacao === "alterar").length ?? 0;

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto pr-1">
      <header>
        <h1 className="text-2xl font-semibold">Perfis</h1>
        <p className="text-sm text-muted-foreground">
          Um perfil desativa vários serviços de uma vez. Você sempre vê a prévia antes de aplicar,
          e um backup automático é criado antes de qualquer mudança.
        </p>
      </header>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        {perfis.map((perfil) => (
          <Card key={perfil.id} className="flex flex-col">
            <CardHeader>
              <div className="flex items-center gap-2">
                {ICONES[perfil.id] ?? <UserCog className="size-5 text-muted-foreground" />}
                <CardTitle>{perfil.nome}</CardTitle>
                {!perfil.embutido && <Badge variant="outline">Personalizado</Badge>}
              </div>
              <CardDescription>{perfil.descricao}</CardDescription>
            </CardHeader>
            <CardContent className="mt-auto flex items-center justify-between gap-2">
              <span className="text-xs text-muted-foreground">
                {plural(perfil.itens.length, "serviço", "serviços")}
              </span>
              <div className="flex gap-2">
                {!perfil.embutido && (
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => setAExcluir(perfil)}
                    aria-label={`Excluir perfil ${perfil.nome}`}
                    title="Excluir perfil"
                  >
                    <Trash2 />
                  </Button>
                )}
                <Button size="sm" onClick={() => void abrirPrevia(perfil)}>
                  Ver prévia
                </Button>
              </div>
            </CardContent>
          </Card>
        ))}
        {perfis.length === 0 && (
          <p className="col-span-full py-10 text-center text-sm text-muted-foreground">
            {carregando ? "Carregando perfis…" : "Nenhum perfil disponível."}
          </p>
        )}
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Salvar seleção atual como perfil</CardTitle>
          <CardDescription>
            Guarda como perfil personalizado todos os serviços cujo tipo de inicialização está
            diferente do estado original — hoje, {plural(alterados, "serviço", "serviços")}.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form
            className="flex gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              void salvarPersonalizado();
            }}
          >
            <Input
              placeholder="Nome do perfil (ex.: Meu notebook)"
              maxLength={40}
              value={nomeNovo}
              onChange={(e) => setNomeNovo(e.target.value)}
            />
            <Button
              type="submit"
              variant="secondary"
              disabled={salvando || alterados === 0 || nomeNovo.trim() === ""}
            >
              {salvando ? <Loader2 className="animate-spin" /> : <Save />}
              Salvar perfil
            </Button>
          </form>
        </CardContent>
      </Card>

      <ConfirmDialog
        aberto={selecionado !== null}
        className="max-w-3xl"
        ocupado={aplicando}
        perigoso={selecionado?.confirmacao_extra}
        titulo={`Aplicar o perfil ${selecionado?.nome ?? ""}?`}
        rotuloConfirmar={
          previa ? `Aplicar ${plural(mudancas, "alteração", "alterações")}` : "Aplicar"
        }
        confirmarDesabilitado={
          previa === null || mudancas === 0 || (selecionado?.confirmacao_extra === true && !ciente)
        }
        descricao={<p>Confira abaixo exatamente o que será alterado neste computador.</p>}
        onCancelar={() => setSelecionado(null)}
        onConfirmar={() => selecionado && void aplicar(selecionado)}
      >
        {previa === null ? (
          <p className="flex items-center justify-center gap-2 py-10 text-sm text-muted-foreground">
            <Loader2 className="size-4 animate-spin" /> Gerando prévia…
          </p>
        ) : (
          <>
            <Previa previa={previa} />
            {mudancas > 0 && (
              <div className="flex flex-col gap-2 rounded-lg border bg-card/50 p-3">
                <Checkbox marcado={criarPonto} onChange={setCriarPonto} disabled={aplicando}>
                  Criar também um ponto de restauração do Windows antes de aplicar (pode levar
                  cerca de um minuto).
                </Checkbox>
                {selecionado?.confirmacao_extra && (
                  <Checkbox marcado={ciente} onChange={setCiente} disabled={aplicando}>
                    <span className="font-medium text-warning">
                      Li a prévia e entendo que recursos como impressão, Bluetooth ou busca podem
                      parar de funcionar até eu restaurar.
                    </span>
                  </Checkbox>
                )}
              </div>
            )}
          </>
        )}
      </ConfirmDialog>

      <ConfirmDialog
        aberto={aExcluir !== null}
        perigoso
        titulo={`Excluir o perfil ${aExcluir?.nome ?? ""}?`}
        rotuloConfirmar="Excluir"
        descricao={<p>O perfil será removido. Os serviços não são alterados.</p>}
        onCancelar={() => setAExcluir(null)}
        onConfirmar={() => aExcluir && void excluir(aExcluir)}
      />
    </div>
  );
}
