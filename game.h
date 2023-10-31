#pragma once
#include "init.h"
#include "entity.h"

enum Estructures {
	FLOOR, WALL
};

enum Operations {
	NONE, SUM, MULTIPLY, ESPECIAL, LEAVE
};

typedef struct {
	char placeholder[25];
	int size;
	bool isHover;

	Coordenades coordenades;
	Coordenades limit;
	Coordenades fontPosition;

	ALLEGRO_FONT* font;
	ALLEGRO_BITMAP* bitmap;
} Button;

typedef struct {
	bool hasColision;
	int type;

	Coordenades position;

	ALLEGRO_BITMAP* path;
} Tile;

struct Map {
	int columns;
	int rows;
	char path[30];
	Tile** tileArr;
	ALLEGRO_BITMAP*** refinedMap;
	bool finish;
	bool error;
	int stage;
} ;

//map
void init_map(struct Map* map);
void draw_map(struct Map* map);


//pages
int menu(struct System* sys);
//int explain(System* sys);
int battle(struct System* sys);
void confront(struct System* sys, struct Entity *player, struct Entity *enemy, int phase);
//int over(System* sys); 


//btn
void drawBtn(Button btn);
void destroyBtn(Button btn);


//utils
int binarySearch(int arr[], int num, int left, int right);
int* intToArr(int num, int index);
int arrToInt(int* arr, int index);
bool isCorrect(int result, int answer);