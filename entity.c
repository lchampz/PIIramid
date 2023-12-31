#include "entity.h"
#include "game.h"
#include <stdio.h>

void init_entity(Entity* entity, ALLEGRO_BITMAP* sprite, ALLEGRO_DISPLAY* display, bool player) {
	entity->alive = YES;
	entity->maxLife = 10;
	entity->lifePoints = entity->maxLife;
	entity->move = UP;
	entity->frame = 0;
	if (player) {
		entity->hitbox.X = SPRITE_CHAR_W;
		entity->hitbox.Y = SPRITE_CHAR_H;
	}
	else {
		entity->hitbox.X = 0;
		entity->hitbox.Y = 0;
	}
	entity->sprite = sprite;
	entity->speed = 5;
	entity->position.X = al_get_display_width(display) / 2 - entity->hitbox.X / 2;
	entity->position.Y = al_get_display_width(display) / 2 - entity->hitbox.Y / 2;
	entity->frameDelay = 8;
	entity->countFrame = 8;
	entity->isMoving = NO;
	entity->animation = IDLE;
}

void checkColision(Entity entity, Map map) {
	Coordenades colision = { .X = NO, .Y = NO };
	int index = 0;
	
	for (int i = 0; map.rows > i; i++) {
		for (int j = 0; map.columns > j; j++) {
			if (map.tileArr[i][j].hasColision) {
				if(map.tileArr[i][j].position.X == entity.position.X || map.tileArr[i][j].position.Y == entity.position.Y)
					printf("Colidiu com o bloco [%d][%d]\n", i, j);
			}
		}
	}
}

void move_entity(Entity* entity, Map map) {
	if (entity->isMoving == YES ) {
		entity->animation = RUNNING;
		switch (entity->move)
		{
		case UP:
			if (entity->position.Y > 0) {
				entity->position.Y -= entity->speed;
			}
			break;
		case DOWN:
			if (entity->position.Y < HEIGHT - entity->hitbox.Y) {
				entity->position.Y += entity->speed;
			}
			break;
		case LEFT:
			if (entity->position.X > 0 - entity->hitbox.X) {
				entity->position.X -= entity->speed;
			}
			break;
		case RIGHT:
			if (entity->position.X < WIDTH - entity->hitbox.X) {
				entity->position.X += entity->speed;
			}
		}
	}
	else entity->animation = IDLE;
	if (map.finish) checkColision(*entity, map);
}

void draw_entity(Entity* entity) {
	
	int fx = entity->frame * entity->hitbox.X;
	int fy = entity->move * entity->hitbox.Y;

	if (entity->animation != IDLE) al_draw_bitmap_region(entity->sprite, fx, fy, entity->hitbox.X, entity->hitbox.Y, entity->position.X, entity->position.Y, 0);
	else al_draw_bitmap_region(entity->sprite, 0, entity->move * entity->hitbox.Y, entity->hitbox.X, entity->hitbox.Y, entity->position.X, entity->position.Y, 0);
}

void destroy_entity(Entity* entity) {
	al_destroy_bitmap(entity->sprite);
}

