#pragma once
#include <allegro5/allegro.h>
#include <allegro5/allegro_image.h>
#include "consts.h"
#include "game.h"

enum Moves {
	 DOWN, UP, LEFT, RIGHT
};
enum Animation {
	RUNNING, IDLE
};

typedef struct Entity {
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

	bool isMoving;
	bool player;
	
	ALLEGRO_BITMAP* sprite;

	Coordenades colision;
} Entity;


void draw_entity(Entity* entity);

void init_entity(Entity* entity, ALLEGRO_BITMAP* sprite, ALLEGRO_DISPLAY* display, bool player);
bool check_entity_colision(Entity player, Entity enemy);
void move_entity(Entity* entity, Map map);

void destroy_entity(Entity* entity);

