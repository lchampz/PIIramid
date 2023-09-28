#include "init.h"

bool verify(System* sys) {
	if (al_init()) {
		sys->display = al_create_display(WIDTH, HEIGHT);
		sys->queue = al_create_event_queue();
		sys->timer = al_create_timer(1.0 / FPS);

		if (sys->display != NULL && sys->queue != NULL && sys->timer != NULL) return NO;
		printf("the queue, display n timer cannot be started");
		return YES;
	}
	printf("the allegro cannot be started");
	return YES;
}

bool verifyModule(bool module, char name[]) {
	if (!module) printf("the module %c cannot be started", *name);
	return !module;
}

System init() {
	System sys;
	sys.running = YES;

	al_set_app_name("PIramid");

	sys.error = verify(&sys);
	
	if (sys.error == YES) return sys;

	sys.menu = NO;
	sys.battle = NO;
	sys.configuration = NO;
	sys.record = NO;

	al_set_window_title(sys.display, NAME);

	sys.error = verifyModule(al_install_keyboard(), "keyboard")
		|| verifyModule(al_install_mouse(), "mouse")
		|| verifyModule(al_init_image_addon(), "image")
		|| verifyModule(al_init_primitives_addon(), "primitive")
		|| verifyModule(al_init_font_addon(), "font")
		|| verifyModule(al_init_ttf_addon(), "ttf");

	if (sys.error == YES) return sys;
	
	sys.font = al_load_font("./assets/font.ttf", 25, 0);

	al_register_event_source(sys.queue, al_get_keyboard_event_source());
	al_register_event_source(sys.queue, al_get_mouse_event_source());
	al_register_event_source(sys.queue, al_get_display_event_source(sys.display));
	al_register_event_source(sys.queue, al_get_timer_event_source(sys.timer));

	al_start_timer(sys.timer);

	sys.error = NO;

	return sys;
}

void destroy(System* sys) {
	if (sys->display != NULL) al_destroy_display(sys->display);
	if (sys->timer != NULL) al_destroy_timer(sys->timer);
	if (sys->font != NULL) al_destroy_font(sys->font);
	if (sys->queue != NULL) al_destroy_event_queue(sys->queue);
}