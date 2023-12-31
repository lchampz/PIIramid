#include "game.h"

int menu(System* sys) {
	bool finished = NO;
	bool draw = YES;

	ALLEGRO_BITMAP* bg = al_load_bitmap("./assets/bg.png");
	ALLEGRO_BITMAP* logo = al_load_bitmap("./assets/logo.png");

	Button btnPlay = {.placeholder = "Jogar"};
	Button btnRecord = { .placeholder = "Record" };
	Button btnConfig = { .placeholder = "Configs" };

	btnPlay.bitmap = al_load_bitmap("./assets/btnBase.png");
	btnPlay.coordenades.X = 50;
	btnPlay.coordenades.Y = 150;
	btnPlay.size = 25;
	btnPlay.font = al_load_font("./assets/font.ttf", btnPlay.size, 0);
	btnPlay.isHover = NO;
	btnPlay.limit.X = btnPlay.coordenades.X + al_get_bitmap_width(btnPlay.bitmap);
	btnPlay.limit.Y = btnPlay.coordenades.Y + al_get_bitmap_height(btnPlay.bitmap);

	btnRecord.bitmap = al_load_bitmap("./assets/btnBase.png");
	btnRecord.coordenades.X = 50;
	btnRecord.coordenades.Y = 250;
	btnRecord.size = 25;
	btnRecord.font = al_load_font("./assets/font.ttf", btnRecord.size, 0);
	btnRecord.isHover = NO;
	btnRecord.limit.X = btnRecord.coordenades.X + al_get_bitmap_width(btnRecord.bitmap);
	btnRecord.limit.Y = btnRecord.coordenades.Y + al_get_bitmap_height(btnRecord.bitmap);

	btnConfig.bitmap = al_load_bitmap("./assets/btnBase.png");
	btnConfig.coordenades.X = 50;
	btnConfig.coordenades.Y = 350;
	btnConfig.size = 25;
	btnConfig.font = al_load_font("./assets/font.ttf", btnConfig.size, 0);
	btnConfig.isHover = NO;
	btnConfig.limit.X = btnConfig.coordenades.X + al_get_bitmap_width(btnConfig.bitmap);
	btnConfig.limit.Y = btnConfig.coordenades.Y + al_get_bitmap_height(btnConfig.bitmap);

	Coordenades mouse = {.X = 0, .Y = 0};

	while (sys->running && !finished && !sys->error && !sys->battle && !sys->configuration && !sys->record) {
		ALLEGRO_EVENT event;

		al_wait_for_event(sys->queue, &event);

		switch (event.type) {
		case ALLEGRO_EVENT_TIMER:
			draw = YES;
			break;
		case ALLEGRO_EVENT_DISPLAY_CLOSE:
			sys->running = NO;
			break;
		case ALLEGRO_EVENT_MOUSE_AXES:
			mouse.X = event.mouse.x;
			mouse.Y = event.mouse.y;

			btnPlay.isHover = ((btnPlay.coordenades.X < mouse.X) && (mouse.X < btnPlay.limit.X)) && ((btnPlay.coordenades.Y < mouse.Y) && (mouse.Y < btnPlay.limit.Y));
			btnRecord.isHover = ((btnRecord.coordenades.X < mouse.X) && (mouse.X < btnRecord.limit.X)) && ((btnRecord.coordenades.Y < mouse.Y) && (mouse.Y < btnRecord.limit.Y));
			btnConfig.isHover = ((btnConfig.coordenades.X < mouse.X) && (mouse.X < btnConfig.limit.X)) && ((btnConfig.coordenades.Y < mouse.Y) && (mouse.Y < btnConfig.limit.Y));
			break;
		case ALLEGRO_EVENT_MOUSE_BUTTON_DOWN:
			mouse.X = event.mouse.x;
			mouse.Y = event.mouse.y;

			if (event.mouse.button & 1) {
				if (btnPlay.isHover) sys->battle = YES;
				if (btnRecord.isHover) sys->record = YES;
				if (btnConfig.isHover) sys->configuration = YES;	
			}

		}
		

		if (draw && al_is_event_queue_empty(sys->queue)) {
			draw = NO;
			al_clear_to_color(HOVER_BLACK);

			al_draw_bitmap(bg, 0, 0, 0);
			al_draw_bitmap(logo, 500, -50, 0);

			al_draw_bitmap(btnPlay.bitmap, btnPlay.coordenades.X, btnPlay.coordenades.Y, 0);
			al_draw_bitmap(btnRecord.bitmap, btnRecord.coordenades.X, btnRecord.coordenades.Y, 0);
			al_draw_bitmap(btnConfig.bitmap, btnConfig.coordenades.X, btnConfig.coordenades.Y, 0);

			if (btnPlay.isHover) al_draw_text(btnPlay.font, HOVER_WHITE, btnPlay.coordenades.X + 48, btnPlay.coordenades.Y + 30, 0, btnPlay.placeholder);
			else al_draw_text(btnPlay.font, HOVER_BLACK, btnPlay.coordenades.X + 48, btnPlay.coordenades.Y + 30, 0, btnPlay.placeholder);

			if (btnRecord.isHover) al_draw_text(btnRecord.font, HOVER_WHITE, btnRecord.coordenades.X + 48, btnRecord.coordenades.Y + 30, 0, btnRecord.placeholder);
			else al_draw_text(btnRecord.font, HOVER_BLACK, btnRecord.coordenades.X + 48, btnRecord.coordenades.Y + 30, 0, btnRecord.placeholder);

			if (btnConfig.isHover) al_draw_text(btnConfig.font, HOVER_WHITE, btnConfig.coordenades.X + 48, btnConfig.coordenades.Y + 30, 0, btnConfig.placeholder);
			else al_draw_text(btnConfig.font, HOVER_BLACK, btnConfig.coordenades.X + 48, btnConfig.coordenades.Y + 30, 0, btnConfig.placeholder);

			al_flip_display();
		}
	}
	destroyBtn(btnPlay);
	destroyBtn(btnRecord);
	destroyBtn(btnConfig);
	al_destroy_bitmap(bg);
	al_destroy_bitmap(logo);
	
	return 0; 
}