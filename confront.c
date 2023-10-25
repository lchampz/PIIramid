#include "game.h"
#include "entity.h"

void confront(struct System* sys, struct Entity *player, struct Entity *enemy) {
	bool finished = NO;
	bool draw = YES;
	int operation = NONE;

	Coordenades mouse;

	Button btnMultiply = { .placeholder = "Multiplicação" };
	btnMultiply.bitmap = al_load_bitmap("./assets/btnBase.png");
	btnMultiply.coordenades.X = 150;
	btnMultiply.coordenades.Y = 850;
	btnMultiply.size = 25;
	btnMultiply.font = al_load_font("./assets/font.ttf", btnMultiply.size, 0);
	btnMultiply.isHover = NO;
	btnMultiply.limit.X = btnMultiply.coordenades.X + al_get_bitmap_width(btnMultiply.bitmap);
	btnMultiply.limit.Y = btnMultiply.coordenades.Y + al_get_bitmap_height(btnMultiply.bitmap);

	Button btnSum = { .placeholder = "Soma" };
	btnSum.bitmap = al_load_bitmap("./assets/btnBase.png");
	btnSum.coordenades.X = 250;
	btnSum.coordenades.Y = 850;
	btnSum.size = 25;
	btnSum.font = al_load_font("./assets/font.ttf", btnSum.size, 0);
	btnSum.isHover = NO;
	btnSum.limit.X = btnSum.coordenades.X + al_get_bitmap_width(btnSum.bitmap);
	btnSum.limit.Y = btnSum.coordenades.Y + al_get_bitmap_height(btnSum.bitmap);

	Button btnEspecial = { .placeholder = "Especial" };
	btnEspecial.bitmap = al_load_bitmap("./assets/btnBase.png");
	btnEspecial.coordenades.X = 350;
	btnEspecial.coordenades.Y = 850;
	btnEspecial.size = 25;
	btnEspecial.font = al_load_font("./assets/font.ttf", btnEspecial.size, 0);
	btnEspecial.isHover = NO;
	btnEspecial.limit.X = btnEspecial.coordenades.X + al_get_bitmap_width(btnEspecial.bitmap);
	btnEspecial.limit.Y = btnEspecial.coordenades.Y + al_get_bitmap_height(btnEspecial.bitmap);

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

				btnSum.isHover = ((btnSum.coordenades.X < mouse.X) && (mouse.X < btnSum.limit.X)) && ((btnSum.coordenades.Y < mouse.Y) && (mouse.Y < btnSum.limit.Y));
				btnMultiply.isHover = ((btnMultiply.coordenades.X < mouse.X) && (mouse.X < btnMultiply.limit.X)) && ((btnMultiply.coordenades.Y < mouse.Y) && (mouse.Y < btnMultiply.limit.Y));
				btnEspecial.isHover = ((btnEspecial.coordenades.X < mouse.X) && (mouse.X < btnEspecial.limit.X)) && ((btnEspecial.coordenades.Y < mouse.Y) && (mouse.Y < btnEspecial.limit.Y));
				break;
			case ALLEGRO_EVENT_MOUSE_BUTTON_DOWN:
				mouse.X = event.mouse.x;
				mouse.Y = event.mouse.y;

				if (event.mouse.button & 1) {
					if (btnSum.isHover) operation = SUM;
					if (btnMultiply.isHover) operation = MULTIPLY;
					if (btnEspecial.isHover) operation = ESPECIAL;
				}
		}

		if (player->lifePoints <= 0) player->alive = NO;
		if (enemy->lifePoints <= 0) enemy->alive = NO;
		
		if (draw && al_is_event_queue_empty(sys->queue)) {
			draw = NO;
			al_clear_to_color(HOVER_BLACK);

			//lifebar
			al_draw_text(sys->font, al_map_rgb(255, 255, 255), 20, 20, 0, "Vida: ");
			al_draw_rectangle(100, 20, 200, 40, al_map_rgb(0, 255, 0), 2);
			al_draw_filled_rectangle(100, 20, player->lifePoints + 100, 40, al_map_rgb(0, 255, 0));

			//lifebar
			al_draw_text(sys->font, al_map_rgb(255, 255, 255), 700, 20, 0, "Inimigo: ");
			al_draw_rectangle(820, 20, 920, 40, al_map_rgb(0, 255, 0), 2);
			al_draw_filled_rectangle(820, 20, enemy->lifePoints + 820, 40, al_map_rgb(0, 255, 0));

			al_draw_filled_rectangle(0, 450, WIDTH, HEIGHT, al_map_rgb(236, 198, 152));

			drawBtn(btnSum);
			
			al_flip_display();
		}
	}
}