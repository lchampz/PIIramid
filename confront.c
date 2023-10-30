#include "game.h"
#include "entity.h"
#include <time.h>

void confront(struct System* sys, struct Entity *player, struct Entity *enemy) {
	bool finished = NO;
	bool draw = YES;
	bool description = YES;
	int operation = NONE;	
	int count = 0;
	int answer = 0;
	char* questionsMultiply[] = { "25 * 2 = ?", "200 * 30 = ?", "(40 * 40) * 3 = ?" };
	char* questionsSum[] = { "25 + 2 = ?", "200 + 30 = ?", "(40 + 40) + 3 = ?" };
	char* questionsEspecial[] = { "fx => 25 * x = 100", "fx => 200 * x = 0", "fx => (40 + 40) * x = 160" };
	int answersMultiply[] = { 50, 6000, 4800 };
	int answersSum[] = { 27, 230, 83};
	int answersEspecial[] = { 4, 0, 2};
	int* arrAnswer = malloc(9 * sizeof(int));
	int rdmIndex = rand() % 2;


	srand(time(NULL));

	ALLEGRO_FONT* titleMonster = al_load_font("./assets/font.ttf", 30, 0);

	Coordenades mouse;
	ALLEGRO_BITMAP* logo = al_load_bitmap("./assets/logo.png");

	Button btnMultiply = { .placeholder = "Multiplicação" };
	btnMultiply.bitmap = al_load_bitmap("./assets/btnBase.png");
	btnMultiply.coordenades.X = 30;
	btnMultiply.coordenades.Y = 600;
	btnMultiply.size = 25;
	btnMultiply.font = al_load_font("./assets/font.ttf", btnMultiply.size, 0);
	btnMultiply.isHover = NO;
	btnMultiply.limit.X = btnMultiply.coordenades.X + al_get_bitmap_width(btnMultiply.bitmap);
	btnMultiply.limit.Y = btnMultiply.coordenades.Y + al_get_bitmap_height(btnMultiply.bitmap);
	btnMultiply.fontPosition.X = 18;
	btnMultiply.fontPosition.Y = 30;

	Button btnSum = { .placeholder = "Soma" };
	btnSum.bitmap = al_load_bitmap("./assets/btnBase.png");
	btnSum.coordenades.X = 330;
	btnSum.coordenades.Y = 600;
	btnSum.size = 25;
	btnSum.font = al_load_font("./assets/font.ttf", btnSum.size, 0);
	btnSum.isHover = NO;
	btnSum.limit.X = btnSum.coordenades.X + al_get_bitmap_width(btnSum.bitmap);
	btnSum.limit.Y = btnSum.coordenades.Y + al_get_bitmap_height(btnSum.bitmap);
	btnSum.fontPosition.X = 48;
	btnSum.fontPosition.Y = 30;

	Button btnEspecial = { .placeholder = "Especial" };
	btnEspecial.bitmap = al_load_bitmap("./assets/btnBase.png");
	btnEspecial.coordenades.X = 630;
	btnEspecial.coordenades.Y = 600;
	btnEspecial.size = 25;
	btnEspecial.font = al_load_font("./assets/font.ttf", btnEspecial.size, 0);
	btnEspecial.isHover = NO;
	btnEspecial.limit.X = btnEspecial.coordenades.X + al_get_bitmap_width(btnEspecial.bitmap);
	btnEspecial.limit.Y = btnEspecial.coordenades.Y + al_get_bitmap_height(btnEspecial.bitmap);
	btnEspecial.fontPosition.X = 42;
	btnEspecial.fontPosition.Y = 30;

	Button btnLeave = { .placeholder = "Fugir" };
	btnLeave.bitmap = al_load_bitmap("./assets/btnBase.png");
	btnLeave.coordenades.X = 1030;
	btnLeave.coordenades.Y = 600;
	btnLeave.size = 25;
	btnLeave.font = al_load_font("./assets/font.ttf", btnLeave.size, 0);
	btnLeave.isHover = NO;
	btnLeave.limit.X = btnLeave.coordenades.X + al_get_bitmap_width(btnLeave.bitmap);
	btnLeave.limit.Y = btnLeave.coordenades.Y + al_get_bitmap_height(btnLeave.bitmap);
	btnLeave.fontPosition.X = 42;
	btnLeave.fontPosition.Y = 30; 

	Button btnAnswer = { .placeholder = "Responder" };
	btnAnswer.bitmap = al_load_bitmap("./assets/btnBase.png");
	btnAnswer.coordenades.X = 720;
	btnAnswer.coordenades.Y = 600;
	btnAnswer.size = 25;
	btnAnswer.font = al_load_font("./assets/font.ttf", btnAnswer.size, 0);
	btnAnswer.isHover = NO;
	btnAnswer.limit.X = btnLeave.coordenades.X + al_get_bitmap_width(btnAnswer.bitmap);
	btnAnswer.limit.Y = btnLeave.coordenades.Y + al_get_bitmap_height(btnAnswer.bitmap);
	btnAnswer.fontPosition.X = 18;
	btnAnswer.fontPosition.Y = 30;

	while (enemy->alive && player->alive && sys->running && !finished && !sys->error && sys->confront) {
		ALLEGRO_EVENT event;

		al_wait_for_event(sys->queue, &event);

		if (event.type == ALLEGRO_EVENT_TIMER) {
			draw = YES;
		}

		switch (event.type) {
			case ALLEGRO_EVENT_DISPLAY_CLOSE:
				finished = YES;
				sys->running = NO;
			break;
			case ALLEGRO_EVENT_MOUSE_AXES:
				mouse.X = event.mouse.x;
				mouse.Y = event.mouse.y;

				btnLeave.isHover = ((btnLeave.coordenades.X < mouse.X) && (mouse.X < btnLeave.limit.X)) && ((btnLeave.coordenades.Y < mouse.Y) && (mouse.Y < btnLeave.limit.Y));
				btnSum.isHover = ((btnSum.coordenades.X < mouse.X) && (mouse.X < btnSum.limit.X)) && ((btnSum.coordenades.Y < mouse.Y) && (mouse.Y < btnSum.limit.Y));
				btnMultiply.isHover = ((btnMultiply.coordenades.X < mouse.X) && (mouse.X < btnMultiply.limit.X)) && ((btnMultiply.coordenades.Y < mouse.Y) && (mouse.Y < btnMultiply.limit.Y));
				btnEspecial.isHover = ((btnEspecial.coordenades.X < mouse.X) && (mouse.X < btnEspecial.limit.X)) && ((btnEspecial.coordenades.Y < mouse.Y) && (mouse.Y < btnEspecial.limit.Y));
				btnAnswer.isHover = ((btnAnswer.coordenades.X < mouse.X) && (mouse.X < btnAnswer.limit.X)) && ((btnAnswer.coordenades.Y < mouse.Y) && (mouse.Y < btnAnswer.limit.Y));
			break;
			case ALLEGRO_EVENT_KEY_DOWN:
				switch (event.keyboard.keycode) {
				case ALLEGRO_KEY_0:
					if (count < 9) {
						arrAnswer[count] = 0;
						count++;
					}
				break;
				case ALLEGRO_KEY_1:
					if (count < 9) {
						arrAnswer[count] = 1;
						count++;
					}
					
				break;
				case ALLEGRO_KEY_2:
					if (count < 9) {
						arrAnswer[count] = 2;
						count++;
					}
				break;
				case ALLEGRO_KEY_3:
					if (count < 9) {
						arrAnswer[count] = 3;
						count++;
					}
				break;
				case ALLEGRO_KEY_4:
					if (count < 9) {
						arrAnswer[count] = 4;
						count++;
					}
				break;
				case ALLEGRO_KEY_5:
					if (count < 9) {
						arrAnswer[count] = 5;
						count++;
					}
				break;
				case ALLEGRO_KEY_6:
					if (count < 9) {
						arrAnswer[count] = 6;
						count++;
					}
				break;
				case ALLEGRO_KEY_7:
					if (count < 9) {
						arrAnswer[count] = 7;
						count++;
					}
					break;
				case ALLEGRO_KEY_8:
					if (count < 9) {
						arrAnswer[count] = 8;
						count++;
					}
					break;
				case ALLEGRO_KEY_9:
					if (count < 9) {
						arrAnswer[count] = 9;
						count++;
					}
					break;
				}
				break;
			case ALLEGRO_EVENT_MOUSE_BUTTON_DOWN:
				mouse.X = event.mouse.x;
				mouse.Y = event.mouse.y;

				if (event.mouse.button & 1) {
					if (btnSum.isHover) {
						operation = SUM;
						description = NO;
					}
					if (btnMultiply.isHover) {
						operation = MULTIPLY;
						description = NO;
					}
					if (btnEspecial.isHover) {
						operation = ESPECIAL;
						description = NO;
					}
					if (btnAnswer.isHover) {
						answer = arrToInt(arrAnswer, count);
						if (operation == MULTIPLY) {
							if (answer == answersMultiply[rdmIndex]) {
								if (enemy->type == MUMMY) enemy->lifePoints -= 30;
								else enemy->lifePoints -= 10;
								answered = NO;
							}
						}
						if (operation == SUM) {
							if (answer == answersSum[rdmIndex]) {
								if (enemy->type == MUMMY) enemy->lifePoints -= 10;
								else enemy->lifePoints -= 30;
								answered = NO;
							}
						}
						if (operation == ESPECIAL) {
							if (answer == answersEspecial[rdmIndex])enemy->lifePoints -= enemy->lifePoints / 2;
							else player->lifePoints -= player->lifePoints / 2;
							answered = NO;
						}
						description = YES;
						operation = NONE;
					}
					if (btnLeave.isHover) operation = LEAVE;
				}
		}

		if (player->lifePoints <= 0) player->alive = NO;
		if (enemy->lifePoints <= 0) enemy->alive = NO;
		if (operation == LEAVE) sys->confront = NO;

		if (draw && al_is_event_queue_empty(sys->queue)) {
			draw = NO;
			al_clear_to_color(HOVER_BLACK);

			//lifebar
			al_draw_text(sys->font, al_map_rgb(255, 255, 255), 20, 20, 0, "Vida: ");
			al_draw_rectangle(100, 20, 200, 40, al_map_rgb(255, 255, 255), 2);
			al_draw_filled_rectangle(100, 20, player->lifePoints + 100, 40, al_map_rgb(0, 255, 0));

			//lifebar
			al_draw_text(sys->font, al_map_rgb(255, 255, 255), 700, 20, 0, "Inimigo: ");
			al_draw_rectangle(820, 20, 920, 40, al_map_rgb(255, 255, 255), 2);
			al_draw_filled_rectangle(820, 20, enemy->lifePoints + 820, 40, al_map_rgb(0, 255, 0));

			al_draw_filled_rectangle(0, 450, WIDTH, HEIGHT, al_map_rgb(236, 198, 152));

			if (description) {
				if (enemy->type == MUMMY) {
					al_draw_text(titleMonster, al_map_rgb(255, 255, 255), 20, 420, 0, "Mumia");
					al_draw_text(sys->font, HOVER_BLACK, 20, 460, 0, DESCRIPTION_MUMMY);
					al_draw_text(sys->font, HOVER_BLACK, 20, 490, 0, DESCRIPTION_MUMMY2);
					al_draw_text(sys->font, HOVER_BLACK, 20, 520, 0, "MULTIPLICACAO");
				}

				if (enemy->type == ZOMBIE) {
					al_draw_text(titleMonster, al_map_rgb(255, 255, 255), 20, 420, 0, "Zumbi");
					al_draw_text(sys->font, HOVER_BLACK, 20, 460, 0, DESCRIPTION_ZOMBIE);
					al_draw_text(sys->font, HOVER_BLACK, 20, 490, 0, DESCRIPTION_ZOMBIE2);
					al_draw_text(sys->font, HOVER_BLACK, 20, 520, 0, DESCRIPTION_ZOMBIE3);
				}

				drawBtn(btnSum);
				drawBtn(btnMultiply);
				drawBtn(btnEspecial);
				drawBtn(btnLeave);
			}
			else {
				al_draw_rectangle(20, 500, 205, 530, al_map_rgb(255, 255, 255), 2);
				drawBtn(btnAnswer);
				if (operation == MULTIPLY) al_draw_text(titleMonster, HOVER_BLACK, 20, 460, 0, questionsMultiply[rdmIndex]);
				if (operation == SUM) al_draw_text(titleMonster, HOVER_BLACK, 20, 460, 0, questionsSum[rdmIndex]);
				if (operation == ESPECIAL) al_draw_text(titleMonster, HOVER_BLACK, 20, 460, 0, questionsEspecial[rdmIndex]);
				for (int i = 0; i < count; i++) {
					al_draw_textf(sys->font, HOVER_BLACK, 25 + (i * 20), 505, 0, "%d", arrAnswer[i]);
				}
			}
			al_flip_display();
		}
	}
	free(arrAnswer);
	if (!enemy->alive) destroy_entity(enemy);
	if (!player->alive) destroy_entity(player);
	destroyBtn(btnSum);
	destroyBtn(btnMultiply);
	destroyBtn(btnEspecial);
	destroyBtn(btnLeave);
	destroyBtn(btnAnswer);
}