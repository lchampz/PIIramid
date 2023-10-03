#pragma once
#include "init.h"

enum Estructures {
	FLOOR, WALL
};

typedef struct {
	char placeholder[25];
	int size;
	bool isHover;

	Coordenades coordenades;
	Coordenades limit;

	ALLEGRO_FONT* font;
	ALLEGRO_BITMAP* bitmap;
} Button;

typedef struct{
	int columns;
	int rows;
	char path[30];
	bool finish;
} Map;

typedef struct {
	char name[15];
	bool hasColision;
	int type;
	
	Coordenades position;

	ALLEGRO_BITMAP* path;
} Tile;

//map
bool init_map(Map* map);
bool load_tile(Tile* tile);


//pages
int menu(System* sys);
int explain(System* sys);
int battle(System* sys);
int over(System* sys); 

//destroyers
void destroyBtn(Button btn);