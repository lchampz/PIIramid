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

typedef struct {
	bool hasColision;
	int type;

	Coordenades position;

	ALLEGRO_BITMAP* path;
} Tile;

typedef struct{
	int columns;
	int rows;
	char path[30];
	Tile** tileArr;
	ALLEGRO_BITMAP*** refinedMap;
	bool finish;
	bool error;
	int stage;
} Map;

//map
void init_map(Map* map);
bool load_tile(Tile* tile);
void draw_map(Map* map);


//pages
int menu(System* sys);
int explain(System* sys);
int battle(System* sys);
int over(System* sys); 

//destroyers
void destroyBtn(Button btn);


//utils
int binarySearch(int arr[], int num, int left, int right);