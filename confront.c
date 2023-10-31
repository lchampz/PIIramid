#include "game.h"
#include "entity.h"
#include <time.h>

char* questionsMultiply[] = { "25 * 2 = ?", "200 * 30 = ?", "(40 * 40) * 3 = ?" };
char* questionsSum[] = { "25 + 2 = ?", "200 + 30 = ?", "(40 + 40) + 3 = ?" };
char* questionsEspecial[] = { "fx => 25 * x = 100", "fx => 200 * x = 0", "fx => (40 + 40) * x = 160" };
int answersMultiply[] = { 50, 6000, 4800 };
int answersSum[] = { 27, 230, 83 };
int answersEspecial[] = { 4, 0, 2 };

struct Confront {
	int count; 
	int answer;
	bool description;
	int operation;
};

void checkQuestion(struct Entity* enemy, struct Entity* player, int* arrAnswer, int rdmIndex, struct Confront* confront) {

	confront->answer = arrToInt(arrAnswer, confront->count);
	if (confront->operation == MULTIPLY) {
		if (confront->answer == answersMultiply[rdmIndex]) {
			if (enemy->type == MUMMY) enemy->lifePoints -= 30;
			else enemy->lifePoints -= 10;
		}
	}
	if (confront->operation == SUM) {
		if (confront->answer == answersSum[rdmIndex]) {
			if (enemy->type == MUMMY) enemy->lifePoints -= 10;
			else enemy->lifePoints -= 30;
		}
	}
	if (confront->operation == ESPECIAL) {
		if (confront->answer == answersEspecial[rdmIndex])enemy->lifePoints -= enemy->lifePoints / 2;
		else player->lifePoints -= player->lifePoints / 2;
	}
	confront->description = YES;
	confront->operation = NONE;
	confront->answer = 0;
	for (int i = 0; confront->count > i; i++) arrAnswer[i] = NULL;
}

void confront(struct System* sys, struct Entity *player, struct Entity *enemy, int phase) {
	bool finished = NO;
	bool draw = YES;
	int* arrAnswer = malloc(9 * sizeof(int));
	int rdmIndex = rand() % 2;
	struct Confront confront = { .answer = 0, .count = 0, .description = YES, .operation = NONE };

	srand(time(NULL));

	ALLEGRO_FONT* titleMonster = al_load_font("./assets/font.ttf", 30, 0);

	Coordenades mouse;

	ALLEGRO_BITMAP* dungeon = al_load_bitmap("./assets/sand_dungeon.png");
	if (phase == 2) dungeon = al_load_bitmap("./assets/bg_dungeon.png");

	ALLEGRO_BITMAP* mummy = al_load_bitmap("./assets/mummy_banner.png");
	ALLEGRO_BITMAP* zombie = al_load_bitmap("./assets/zombie_banner.png");

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
	btnAnswer.coordenades.X = 820;
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
					if (confront.count < 9) {
						arrAnswer[confront.count] = 0;
						confront.count++;
					}
				break;
				case ALLEGRO_KEY_1:
					if (confront.count < 9) {
						arrAnswer[confront.count] = 1;
						confront.count++;
					}
					
				break;
				case ALLEGRO_KEY_2:
					if (confront.count < 9) {
						arrAnswer[confront.count] = 2;
						confront.count++;
					}
				break;
				case ALLEGRO_KEY_3:
					if (confront.count < 9) {
						arrAnswer[confront.count] = 3;
						confront.count++;
					}
				break;
				case ALLEGRO_KEY_4:
					if (confront.count < 9) {
						arrAnswer[confront.count] = 4;
						confront.count++;
					}
				break;
				case ALLEGRO_KEY_5:
					if (confront.count < 9) {
						arrAnswer[confront.count] = 5;
						confront.count++;
					}
				break;
				case ALLEGRO_KEY_6:
					if (confront.count < 9) {
						arrAnswer[confront.count] = 6;
						confront.count++;
					}
				break;
				case ALLEGRO_KEY_7:
					if (confront.count < 9) {
						arrAnswer[confront.count] = 7;
						confront.count++;
					}
					break;
				case ALLEGRO_KEY_8:
					if (confront.count < 9) {
						arrAnswer[confront.count] = 8;
						confront.count++;
					}
					break;
				case ALLEGRO_KEY_9:
					if (confront.count < 9) {
						arrAnswer[confront.count] = 9;
						confront.count++;
					}
					break;
				case ALLEGRO_KEY_BACKSPACE:
					
					if (confront.count > 0) confront.count--;
					arrAnswer[confront.count] = NULL;
					
				break;
				case ALLEGRO_KEY_ENTER:	
					if(confront.count != 0) checkQuestion(enemy, player, arrAnswer, rdmIndex, &confront);
				break;
				}
				break;
			case ALLEGRO_EVENT_MOUSE_BUTTON_DOWN:
				mouse.X = event.mouse.x;
				mouse.Y = event.mouse.y;

				if (event.mouse.button & 1) {
					if (btnSum.isHover && confront.description) {
						confront.operation = SUM;
						confront.description = NO;
					}
					if (btnMultiply.isHover && confront.description) {
						confront.operation = MULTIPLY;
						confront.description = NO;
					}
					if (btnEspecial.isHover && confront.description) {
						confront.operation = ESPECIAL;
						confront.description = NO;
					}
					if (btnAnswer.isHover) {
						if(confront.count != 0) checkQuestion(enemy, player, arrAnswer, rdmIndex, &confront);
					}
					if (btnLeave.isHover) confront.operation = LEAVE;
				}
		}

		if (player->lifePoints <= 0) player->alive = NO;
		if (enemy->lifePoints <= 0) enemy->alive = NO;
		if (confront.operation == LEAVE) sys->confront = NO;

		if (draw && al_is_event_queue_empty(sys->queue)) {
			draw = NO;
			al_draw_bitmap(dungeon, 0,0, 0);

			//lifebar
			al_draw_text(sys->font, al_map_rgb(255, 255, 255), 20, 20, 0, "Vida: ");
			al_draw_rectangle(100, 20, 200, 40, al_map_rgb(255, 255, 255), 2);
			al_draw_filled_rectangle(100, 20, player->lifePoints + 100, 40, al_map_rgb(0, 255, 0));

			//lifebar
			al_draw_text(sys->font, al_map_rgb(255, 255, 255), 700, 20, 0, "Inimigo: ");
			al_draw_rectangle(820, 20, 920, 40, al_map_rgb(255, 255, 255), 2);
			al_draw_filled_rectangle(820, 20, enemy->lifePoints + 820, 40, al_map_rgb(0, 255, 0));

			if (enemy->type == MUMMY) {
				al_draw_bitmap(mummy, 800, 130, 0);
			}
			if (enemy->type == ZOMBIE) {
				al_draw_bitmap(zombie, 800, 170, 0);
			}

			al_draw_filled_rectangle(0, 450, WIDTH, HEIGHT, al_map_rgb(236, 198, 152));

			if (confront.description) {
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
				if (confront.operation == MULTIPLY) al_draw_text(titleMonster, HOVER_BLACK, 20, 460, 0, questionsMultiply[rdmIndex]);
				if (confront.operation == SUM) al_draw_text(titleMonster, HOVER_BLACK, 20, 460, 0, questionsSum[rdmIndex]);
				if (confront.operation == ESPECIAL) al_draw_text(titleMonster, HOVER_BLACK, 20, 460, 0, questionsEspecial[rdmIndex]);
				for (int i = 0; i < confront.count; i++) al_draw_textf(sys->font, HOVER_BLACK, 25 + (i * 20), 505, 0, "%d", arrAnswer[i]);
			}
			al_flip_display();
		}
	}
	free(arrAnswer);
	destroyBtn(btnSum);
	destroyBtn(btnMultiply);
	destroyBtn(btnEspecial);
	destroyBtn(btnLeave);
	destroyBtn(btnAnswer);
	al_destroy_bitmap(mummy);
	al_destroy_bitmap(dungeon);
}