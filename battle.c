#include "game.h"
#include "entity.h"

int battle(System* sys) {
	bool finished = NO;
	bool draw = YES;
	bool intro = NO;

	Entity player;
	Entity enemies;

	ALLEGRO_BITMAP* bg = al_load_bitmap("./assets/bg.png");
	ALLEGRO_BITMAP* playerSprite = al_load_bitmap("./assets/sprite.png");
	ALLEGRO_BITMAP* enemySprite = al_load_bitmap("./assets/sprite.png");

	Button btnPlay = { .placeholder = "Entendido!" };
	btnPlay.bitmap = al_load_bitmap("./assets/btnBase.png");
	btnPlay.coordenades.X = 550;
	btnPlay.coordenades.Y = 850;
	btnPlay.size = 25;
	btnPlay.font = al_load_font("./assets/font.ttf", btnPlay.size, 0);
	btnPlay.isHover = NO;
	btnPlay.limit.X = btnPlay.coordenades.X + al_get_bitmap_width(btnPlay.bitmap);
	btnPlay.limit.Y = btnPlay.coordenades.Y + al_get_bitmap_height(btnPlay.bitmap);

	Coordenades mouse = { .X = 0, .Y = 0 };

	init_entity(&player, playerSprite, sys->display, YES);
	init_entity(&enemies, enemySprite, sys->display, NO);

	while (player.alive && sys->running && !finished && !sys->error) {
		ALLEGRO_EVENT event;

		al_wait_for_event(sys->queue, &event);

		if (event.type == ALLEGRO_EVENT_TIMER) {
			draw = YES;

			move_entity(&player);
		}

		if (++player.countFrame >= player.frameDelay) {
			player.frame++;
			if (player.frame >= 4) player.frame = 0;
			player.countFrame = 0;
		}

		switch (event.type) {
		case ALLEGRO_EVENT_DISPLAY_CLOSE:
			finished = YES;
			sys->running = NO;
		break;
		case ALLEGRO_EVENT_KEY_DOWN:
			switch (event.keyboard.keycode) {
				case ALLEGRO_KEY_W:
				case ALLEGRO_KEY_UP:
					player.move = UP;
					player.isMoving = YES;
					printf("UP\n");
				break;
				case ALLEGRO_KEY_S:
				case ALLEGRO_KEY_DOWN:
					player.move = DOWN;
					player.isMoving = YES;
					printf("DOWN\n");
				break;
				case ALLEGRO_KEY_A:
				case ALLEGRO_KEY_LEFT:
					player.move = LEFT;
					player.isMoving = YES;
					printf("LEFT\n");
				break;
				case ALLEGRO_KEY_D:
				case ALLEGRO_KEY_RIGHT:
					player.move = RIGHT;
					player.isMoving = YES;
					printf("RIGHT\n");
				break;
			}
		break;
		case ALLEGRO_EVENT_KEY_UP:
			player.isMoving = NO;
			printf("Cancelando\n");
		break;
		case ALLEGRO_EVENT_MOUSE_AXES:
			mouse.X = event.mouse.x;
			mouse.Y = event.mouse.y;

			btnPlay.isHover = ((btnPlay.coordenades.X < mouse.X) && (mouse.X < btnPlay.limit.X)) && ((btnPlay.coordenades.Y < mouse.Y) && (mouse.Y < btnPlay.limit.Y));
			break;
		case ALLEGRO_EVENT_MOUSE_BUTTON_DOWN:
			mouse.X = event.mouse.x;
			mouse.Y = event.mouse.y;

			if (event.mouse.button & 1) {
				if (btnPlay.isHover) intro = NO;
			}
		}

		if (draw && al_is_event_queue_empty(sys->queue)) {
			draw = NO;

			if (intro) {
				al_clear_to_color(al_map_rgb(0, 0, 0));
				al_draw_text(sys->font, al_map_rgb(255, 255, 255), 200, 200, 0, "Voce");
				

				al_draw_bitmap(btnPlay.bitmap, btnPlay.coordenades.X, btnPlay.coordenades.Y, 0);
				if (btnPlay.isHover) al_draw_text(btnPlay.font, HOVER_WHITE, btnPlay.coordenades.X + 48, btnPlay.coordenades.Y + 30, 0, btnPlay.placeholder);
				else al_draw_text(btnPlay.font, HOVER_BLACK, btnPlay.coordenades.X + 48, btnPlay.coordenades.Y + 30, 0, btnPlay.placeholder);
			}
			else {
				al_draw_bitmap(bg, 0, 0, 0);

				draw_entity(&player);

				al_draw_text(sys->font, al_map_rgb(255, 255, 255), 20, 20, 0, "Vida: ");
				al_draw_rectangle(100, 20, 220, 40, al_map_rgb(0, 255, 0), 2);

				al_draw_filled_rectangle(100, 20, player.lifePoints + 100, 40, al_map_rgb(0, 255, 0));

			}
			
			al_flip_display();
		}
	}
	destroy_entity(&enemies);
	destroy_entity(&player);
	al_destroy_bitmap(bg);

	return !player.alive;
}