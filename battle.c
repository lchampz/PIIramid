#include "game.h"
#include "entity.h"

int battle(struct System* sys) {
	sys->confront = NO;
	bool finished = NO;
	bool draw = YES;
	bool intro = YES;
	int phase = 1;
	int enemySteps = 0;
	
	struct Entity player;
	struct Entity mummy;
	struct Entity zombie;

	ALLEGRO_BITMAP* bg = al_load_bitmap("./assets/bg.png");
	ALLEGRO_BITMAP* playerSprite = al_load_bitmap("./assets/sprite.png");
	ALLEGRO_BITMAP* mummySprite = al_load_bitmap("./assets/enemy2.png");
	ALLEGRO_FONT* title = al_load_font("./assets/font.ttf", 30, 0);

	player.type = PLAYER;
	mummy.type = MUMMY;
	zombie.type = ZOMBIE;
	
	Button btnPlay = { .placeholder = "Vamos la!" };
	btnPlay.bitmap = al_load_bitmap("./assets/btnBase.png");
	btnPlay.coordenades.X = 570;
	btnPlay.coordenades.Y = 430;
	btnPlay.size = 25;
	btnPlay.font = al_load_font("./assets/font.ttf", btnPlay.size, 0);
	btnPlay.isHover = NO;
	btnPlay.limit.X = btnPlay.coordenades.X + al_get_bitmap_width(btnPlay.bitmap);
	btnPlay.limit.Y = btnPlay.coordenades.Y + al_get_bitmap_height(btnPlay.bitmap);
	btnPlay.fontPosition.X = 18;
	btnPlay.fontPosition.Y = 30;

	Coordenades mouse = { .X = 0, .Y = 0 };
	struct Map map = { .path = "./assets/map.txt", .finish = NO, .error = NO};

	init_entity(&player, playerSprite, sys->display, YES);
	init_entity(&mummy, mummySprite, sys->display, NO);
	init_entity(&zombie, playerSprite, sys->display, YES);

	mummy.position.X = 600;
	mummy.speed = 2;

	zombie.position.X = 600;
	zombie.speed = 2;
	
	ALLEGRO_BITMAP* mapBitmap = al_load_bitmap("./assets/map.png");

	while (player.alive && sys->running && !finished && !sys->error) {
		ALLEGRO_EVENT event;

		al_wait_for_event(sys->queue, &event);

		if (event.type == ALLEGRO_EVENT_TIMER) {
			draw = YES;
			move_entity(&player);
			move_entity(&mummy);
			move_entity(&zombie);
			if(phase == 1) sys->confront = check_entity_colision(player, mummy);
			if(phase == 2) sys->confront = check_entity_colision(player, zombie);
		}

		if (++player.countFrame >= player.frameDelay) {
			player.frame++;
			if (player.frame >= 4) player.frame = 0;
			player.countFrame = 0;
		}

		if (phase == 2) {
			if (++zombie.countFrame >= zombie.frameDelay) {
				zombie.frame++;
				if (zombie.frame >= 4) zombie.frame = 0;
				zombie.countFrame = 0;
			}
		}
		
		if (phase == 1) {
			if (++mummy.countFrame >= mummy.frameDelay) {
				mummy.frame++;
				if (mummy.frame >= 3) mummy.frame = 0;
				mummy.countFrame = 0;
			}
		}
		
		switch (event.type) {
		case ALLEGRO_EVENT_DISPLAY_CLOSE:
			finished = YES;
			sys->running = NO;
		break;
		case ALLEGRO_EVENT_KEY_DOWN:
			switch (event.keyboard.keycode) {
				case ALLEGRO_KEY_A:
				case ALLEGRO_KEY_LEFT:
					player.move = LEFT;
					player.isMoving = YES;
					break;
				case ALLEGRO_KEY_D:
				case ALLEGRO_KEY_RIGHT:
					player.move = RIGHT;
					player.isMoving = YES;
					break;
			}
		break;
		case ALLEGRO_EVENT_KEY_UP:
			player.isMoving = NO;
			break;
		case ALLEGRO_EVENT_MOUSE_AXES:
			mouse.X = event.mouse.x;
			mouse.Y = event.mouse.y;

			btnPlay.isHover = ((btnPlay.coordenades.X < mouse.X) && (mouse.X < btnPlay.limit.X)) && ((btnPlay.coordenades.Y < mouse.Y) && (mouse.Y < btnPlay.limit.Y));
			break;
		case ALLEGRO_EVENT_MOUSE_BUTTON_DOWN:
			mouse.X = event.mouse.x;
			mouse.Y = event.mouse.y;

			if (event.mouse.button &  1) {
				if (btnPlay.isHover) intro = NO;
			}
		}

		if (phase == 1) mummy.isMoving = YES;
		if (phase == 2) zombie.isMoving = YES;

		if (enemySteps == 150) {
			if (phase == 1) mummy.move = RIGHT_MUMMY;
			if (phase == 2) zombie.move = RIGHT;
		}
		if (enemySteps == 0) {
			if (phase == 1) mummy.move = LEFT_MUMMY;
			if (phase == 2) zombie.move = LEFT;
		}
		
		if (phase == 1) {
			if (mummy.move == LEFT_MUMMY) enemySteps++;
			else enemySteps--;
		}
		
		if (phase == 2) {
			if (zombie.move == LEFT) enemySteps++;
			else enemySteps--;
		}
		
		if (player.position.X > 1000) {
			phase++;
			enemySteps = 0;
			player.position.X = 0;
		}

		if (draw && al_is_event_queue_empty(sys->queue)) {
			draw = NO;
			al_clear_to_color(HOVER_BLACK);

			if (intro) {
				al_draw_text(title, al_map_rgb(255, 255, 255), 460, 150, 0, "Chamado da Aventura!");
				al_draw_filled_rectangle(200, 200, 1150, 480, al_map_rgb(236, 198, 152));
				
				al_draw_text(sys->font, HOVER_BLACK, 215, 220, 0, INTRO);
				al_draw_text(sys->font, HOVER_BLACK, 215, 250, 0, INTRO2);
				al_draw_text(sys->font, HOVER_BLACK, 215, 280, 0, INTRO3);
				al_draw_text(sys->font, HOVER_BLACK, 215, 310, 0, INTRO4);
				al_draw_text(sys->font, HOVER_BLACK, 500, 380, 0, INTRO5);
				
				drawBtn(btnPlay);
			}
			else {
				if (sys->confront && (phase == 1)) confront(sys, &player, &mummy, phase);
				if (sys->confront && (phase == 2)) confront(sys, &player, &zombie, phase);
				if (phase == 1) {
					al_draw_bitmap(mapBitmap, 0, 0, 0);
					if (player.alive) draw_entity(&player);
					if (mummy.alive) draw_entity(&mummy);

					al_draw_text(sys->font, al_map_rgb(255, 255, 255), 20, 20, 0, "Vida: ");
					al_draw_rectangle(100, 20, 200, 40, al_map_rgb(255, 255, 255), 2);

					al_draw_filled_rectangle(100, 20, player.lifePoints + 100, 40, al_map_rgb(0, 255, 0));
				}
				if (phase == 2) {
					al_draw_bitmap(mapBitmap, 0, 0, 0);
					if (player.alive) draw_entity(&player);
					if (zombie.alive) draw_entity(&zombie);

					al_draw_text(sys->font, al_map_rgb(255, 255, 255), 20, 20, 0, "Vida: ");
					al_draw_rectangle(100, 20, 200, 40, al_map_rgb(255, 255, 255), 2);

					al_draw_filled_rectangle(100, 20, player.lifePoints + 100, 40, al_map_rgb(0, 255, 0));
				}
				if (phase == 3) {

				}
			}
			
			
			
			al_flip_display();
		}
	}
	if (!mummy.alive) destroy_entity(&mummy);
	destroyBtn(btnPlay);
	if (!player.alive) destroy_entity(&player);
	al_destroy_bitmap(bg);

	return !player.alive;
}