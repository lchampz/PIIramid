#pragma once
#include "init.h"

typedef struct {
	char placeholder[25];
	int size;
	bool isHover;

	Coordenades coordenades;
	Coordenades limit;

	ALLEGRO_FONT* font;
	ALLEGRO_BITMAP* bitmap;
} Button;

int menu(System* sys);
int explain(System* sys);
int battle(System* sys);
int over(System* sys); 

void destroyBtn(Button btn);