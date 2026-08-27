//! RFC-005: fluxo padrão do jogador — substitui o mapa livre
//! (`OverworldScene`) por uma sequência linear e fixa dos 7 duelos. Monta o
//! `DuelScene` da fase atual (`save.current_phase`, via o registro central
//! `monsters::PHASES`) e o roda diretamente, sem `Entity`/mapa/movimento.
//! `DuelScene` em si não muda nada — só o que existe *ao redor* dele.

use macroquad::prelude::*;

use crate::assets::Assets;
use crate::inventory::{self, SaveData};
use crate::monsters::{MonsterState, PHASES};
use crate::scenes::duel::{DuelOutcome, DuelScene};
use crate::scenes::Transition;
use crate::ui::theme;
use crate::world::entity::{Entity, Kind};

/// Um duelo de fase precisa de um monstro real; "todas as fases já
/// vencidas" (`save.current_phase >= PHASES.len()`) não tem um `MonsterSpec`
/// pra montar. Isso pode acontecer se o jogador clicar "Continuar" de novo
/// depois da vitória completa (o save persiste `current_phase == 7`, ver
/// crítica de especificação na entrega) — em vez de indexar fora do array,
/// `PhaseScene` nasce já em `Complete` e devolve a mesma
/// `Transition::GoToGameOver { won: true, .. }` no primeiro `update()`, sem
/// duelo nenhum de fato acontecer.
enum Inner {
    Active { player: Entity, foe_kind: Kind, monster: Box<MonsterState>, duel: Box<DuelScene> },
    Complete,
}

pub struct PhaseScene {
    inner: Inner,
    save: SaveData,
}

impl PhaseScene {
    /// QA (ALTO-2): delega pra `DuelScene` -- `main.rs` consulta isto antes
    /// de tratar `ESC` como toggle de pausa (regra 2 da RFC-019), pra que um
    /// overlay modal aberto (CARREGAR/ENSAIAR/escolha de função) tenha
    /// prioridade sobre a pausa nesse frame, em vez do `ESC` ser consumido
    /// antes de `DuelScene::update()` sequer rodar.
    pub fn has_modal_overlay_open(&self) -> bool {
        match &self.inner {
            Inner::Active { duel, .. } => duel.has_modal_overlay_open(),
            Inner::Complete => false,
        }
    }

    pub fn new(save: SaveData) -> Self {
        let inner = match PHASES.get(save.current_phase) {
            Some((kind, spec_fn)) => {
                // Sem mapa: a posição nunca é lida (nada anda, nada
                // colide) — só existe porque `DuelScene::draw` recebe
                // `&Entity` para ler `life_points`/`max_life` (regra 4: sem
                // Entity/mapa/movimento *de jogo*, mas o tipo em si é
                // reaproveitado como "o jogador" pro dossiê de vida).
                let mut player = Entity::new(Kind::Player, true);
                // RFC-025 regra 5: a vida atravessa as fases -- restaura o
                // que `save.player_life` guarda (`None` = vida cheia, já é
                // o que `Entity::new` produz, então não há nada a fazer
                // nesse caso). Piso em 1 e teto em `max_life`: um save
                // corrompido/editado a mão que guarde <= 0 não pode nascer
                // a fase já morto (o jogo não tem um estado de "duelo que
                // começa perdido").
                if let Some(life) = save.player_life {
                    player.life_points = life.clamp(1, player.max_life);
                }
                let is_final_phase = save.current_phase + 1 >= PHASES.len();
                Inner::Active { player, foe_kind: *kind, monster: Box::new(MonsterState::new(spec_fn())), duel: Box::new(DuelScene::new(is_final_phase)) }
            }
            None => Inner::Complete,
        };
        PhaseScene { inner, save }
    }

    pub fn update(&mut self) -> Option<Transition> {
        match &mut self.inner {
            Inner::Complete => Some(Transition::GoToGameOver { won: true, turns: 0, player_hp: 0, last_drop: None }),
            Inner::Active { player, monster, duel, .. } => {
                let outcome = duel.update(player, monster, &mut self.save);
                match outcome {
                    // RFC-005 regra 5: Won -> incrementa e sempre persiste
                    // (mesmo padrão de `OverworldScene::update`, que também
                    // salva tanto no caso "ainda há inimigo" quanto no de
                    // vitória completa da expedição). < 7 -> volta ao menu
                    // (não-objetivo 4: sem encadeamento automático); >= 7 ->
                    // vitória completa da pirâmide.
                    Some(DuelOutcome::Won) => {
                        self.save.current_phase += 1;
                        // RFC-025 regra 5/6: persiste a vida com que o
                        // jogador terminou o duelo, já com a recuperação
                        // parcial entre fases aplicada (`inventory::
                        // recovered_player_life`) — é isso que faz a
                        // próxima fase começar desgastada, mas não sem
                        // chance nenhuma.
                        self.save.player_life = Some(inventory::recovered_player_life(player.life_points, player.max_life));
                        // RFC-028, regra 2/3: só o braço `Won` insere o
                        // despojo temático do monstro (`monster.spec.drop`,
                        // `monsters/data.rs`) na `Bag` — nenhum custo em
                        // ciclos, é estado de progressão de campanha, no
                        // mesmo espírito de `player_life` acima. Derrota e
                        // fuga (braços abaixo) nunca chamam esta função.
                        let drop_label = inventory::apply_phase_victory_drop(&mut self.save, &monster.spec.drop);
                        // RFC-029, regra 4/5: avalia o duelo que acabou de
                        // ser vencido (turnos + ciclos médios, só dados que
                        // `DuelScene` já produz) contra o recorde salvo
                        // daquele monstro específico e persiste se for
                        // melhor. `grade_line` some do texto (só o despojo
                        // aparece) se por algum motivo não houver script de
                        // referência calibrado — nunca inventa nota.
                        let grade_line = crate::grade::apply_duel_result(&mut self.save.best_result, &monster.spec, duel.turns_played(), duel.total_cycles_used())
                            .map(|eval| {
                                if eval.is_new_record {
                                    format!("Nota: {} (novo recorde!)", eval.grade.label())
                                } else {
                                    format!("Nota: {}", eval.grade.label())
                                }
                            });
                        let feedback = match grade_line {
                            Some(line) => format!("{drop_label}\n{line}"),
                            None => drop_label,
                        };
                        self.save.save();
                        if self.save.current_phase >= PHASES.len() {
                            Some(Transition::GoToGameOver { won: true, turns: duel.turn(), player_hp: player.life_points, last_drop: Some(feedback) })
                        } else {
                            Some(Transition::GoToMenu { last_drop: Some(feedback) })
                        }
                    }
                    // regra 5/7: derrota já levava ao GameOver de derrota
                    // (comportamento pré-existente, agora alcançável de
                    // verdade jogando normalmente). `player_life` volta a
                    // `None` (vida cheia) em vez de persistir a vida com
                    // que o jogador morreu (<= 0) — sem isso, "TENTAR DE
                    // NOVO" (`scenes/gameover.rs`) recarregaria a mesma
                    // fase com o jogador já morto, um soft-lock. Não é
                    // punição de progresso (não-objetivo 2: a fase em si
                    // não muda) — é só o que permite a retentativa
                    // acontecer de verdade.
                    Some(DuelOutcome::Lost) => {
                        self.save.player_life = None;
                        self.save.save();
                        Some(Transition::GoToGameOver { won: false, turns: duel.turn(), player_hp: player.life_points, last_drop: None })
                    }
                    // regra 5 + não-objetivo 5: fugir não perde nem avança
                    // progresso — salva (current_phase intocado) e volta ao
                    // menu. Diferente de `OverworldScene`, não há mapa pra
                    // empurrar o jogador de volta (fix do B-007 não se
                    // aplica: não existe overlap de AABB pra limpar aqui).
                    // A vida persiste exatamente como está (sem recuperação
                    // parcial, que é só regra 6, "vencer uma fase" — fugir
                    // não venceu nada) — reentrar na mesma fase retoma
                    // desgastado, não do zero nem já morto.
                    Some(DuelOutcome::Fled) => {
                        self.save.player_life = Some(player.life_points.max(1));
                        self.save.save();
                        Some(Transition::GoToMenu { last_drop: None })
                    }
                    None => None,
                }
            }
        }
    }

    pub fn draw(&self, assets: &Assets) {
        match &self.inner {
            // Só visível no frame em que `PhaseScene` nasce Complete, antes
            // do primeiro `update()` trocar de cena (ver main.rs: a cena
            // nova é desenhada no mesmo frame em que nasce, antes de rodar
            // seu próprio `update()`) — nunca fica na tela por mais que um
            // frame, mas precisa de algo seguro pra desenhar mesmo assim.
            Inner::Complete => {
                clear_background(theme::TUMBA);
                draw_text_ex(
                    "PIRAMIDE CONCLUIDA",
                    40.0,
                    60.0,
                    TextParams { font: Some(&assets.font_title), font_size: theme::TITLE_MD, color: theme::OURO, ..Default::default() },
                );
            }
            Inner::Active { player, foe_kind, monster, duel } => {
                duel.draw(assets, player, monster, *foe_kind, &self.save, Some(self.save.current_phase))
            }
        }
    }
}
