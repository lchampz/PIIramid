#include "game.h"
#include "entity.h"

void confront(System* sys, Entity *player, Entity *enemy) {
	bool finished = NO;
	bool draw = YES;

	while (enemy->alive && player->alive && sys->running && !finished && !sys->error && sys->confront) {
		ALLEGRO_EVENT event;

		al_wait_for_event(sys->queue, &event);

		if (event.type == ALLEGRO_EVENT_TIMER) {
			draw = YES;
			player->lifePoints--;
		}

		switch (event.type) {
			case ALLEGRO_EVENT_DISPLAY_CLOSE:
				finished = YES;
				sys->running = NO;
			break;
		}

		if (player->lifePoints <= 0) player->alive = NO;
		if (enemy->lifePoints <= 0) enemy->alive = NO;
		
		if (draw && al_is_event_queue_empty(sys->queue)) {
			draw = NO;
			al_clear_to_color(HOVER_BLACK);

			al_draw_text(sys->font, al_map_rgb(255, 255, 255), 20, 20, 0, "Vida: ");
			al_draw_text(sys->font, al_map_rgb(255, 255, 255), 200, 20, 0, "Inimigo: ");
				
			al_flip_display();
		}
	}
}