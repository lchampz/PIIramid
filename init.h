#pragma once
#include <stdlib.h>
#include <stdio.h>
#include <stdbool.h>
#include <allegro5/allegro.h>
#include <allegro5/allegro_image.h>
#include <allegro5\allegro_font.h>
#include <allegro5\allegro_ttf.h>
#include <allegro5\allegro_primitives.h>
#include "consts.h"
#include "game.h"


struct System{
	ALLEGRO_DISPLAY* display;
	ALLEGRO_EVENT_QUEUE* queue;
	ALLEGRO_TIMER* timer;
	ALLEGRO_FONT* font;
	bool menu;
	bool record;
	bool battle;
	bool configuration;
	bool confront;
	bool running;
	bool error;
};


//methods
struct System init();
bool verify(struct System* sys);
void destroy(struct System* sys);