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


typedef struct {
	ALLEGRO_DISPLAY* display;
	ALLEGRO_EVENT_QUEUE* queue;
	ALLEGRO_TIMER* timer;
	ALLEGRO_FONT* font;
	bool menu;
	bool record;
	bool configuration;
	bool running;
	bool error;
} System;


//methods
System init();
bool verify(System* sys);
void destroy(System* sys);