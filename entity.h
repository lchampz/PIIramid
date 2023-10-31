#pragma once
#include <allegro5/allegro.h>
#include <allegro5/allegro_image.h>
#include "consts.h"
#include "game.h"

enum Moves {
	 DOWN, UP, LEFT, RIGHT
};

enum MovesMummy {
	LEFT_MUMMY = 3, RIGHT_MUMMY = 1
};
enum Animation {
	RUNNING, IDLE
};
enum TypesOfEntities {
	PLAYER, ZOMBIE, MUMMY
};

struct Entity {
	bool alive;
	int lifePoints;
	int maxLife;
	double speed;

	Coordenades hitbox;
	Coordenades position;

	int frame;
	int move;
	int frameDelay;
	int countFrame;
	int animation;
	int type;

	bool isMoving;
	bool player;
	
	ALLEGRO_BITMAP* sprite;

	Coordenades colision;
};


void draw_entity(struct Entity* entity);

void init_entity(struct Entity* entity, ALLEGRO_BITMAP* sprite, ALLEGRO_DISPLAY* display, bool player);
bool check_entity_colision(struct Entity player, struct Entity enemy);
void move_entity(struct Entity* entity);

void destroy_entity(struct Entity* entity);

