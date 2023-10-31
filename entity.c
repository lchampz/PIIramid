#include "entity.h"
#include "game.h"
#include <stdio.h>

void init_entity(struct Entity* entity, ALLEGRO_BITMAP* sprite, ALLEGRO_DISPLAY* display, bool player) {
	entity->alive = YES;
	entity->maxLife = 100;
	entity->lifePoints = entity->maxLife;
	entity->move = UP;
	entity->frame = 0;
	if (entity->type == PLAYER) {
		entity->hitbox.X = SPRITE_CHAR_W;
		entity->hitbox.Y = SPRITE_CHAR_H;
	}
	if (entity->type == MUMMY) {
		entity->hitbox.X = SPRITE_MUMMY_W;
		entity->hitbox.Y = SPRITE_MUMMY_H;
	}
	entity->sprite = sprite;
	entity->speed = 5;
	entity->position.X = 30;
	entity->position.Y = (al_get_display_width(display) / 2 - entity->hitbox.Y / 2) - 40;
	entity->frameDelay = 8;
	entity->countFrame = 8;
	if (entity->type == MUMMY) {
		entity->frameDelay = 4;
		entity->countFrame = 4;
	}
	entity->isMoving = NO;
	entity->animation = IDLE;
	entity->colision.X = NO;
	entity->colision.Y = NO;
}

bool check_entity_colision(struct Entity player, struct Entity enemy) {
	if (player.position.X == enemy.position.X || player.position.Y == enemy.position.Y) return YES;
	return NO;
}

void move_entity(struct Entity* entity, struct Map map) {
	if (entity->isMoving == YES && entity->type == PLAYER) {
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
			if (entity->type == PLAYER) {
				if (entity->position.X > 0 - entity->hitbox.X) {
					entity->position.X -= entity->speed;
				}
			}
			break;
		case RIGHT:
			if (entity->type == PLAYER) {
				if (entity->position.X < WIDTH - entity->hitbox.X) {
					entity->position.X += entity->speed;
				}
			}
		}
	}
	else entity->animation = IDLE;
	if (entity->isMoving == YES && entity->type == MUMMY) {
		entity->animation = RUNNING;
		switch (entity->move)
		{
		case LEFT_MUMMY:
			if (entity->position.X < WIDTH - entity->hitbox.X) {
				entity->position.X -= entity->speed;
			}
			break;
		case RIGHT_MUMMY:
			if (entity->position.Y > 0) {
				entity->position.X += entity->speed;
			}
			break;
		};
	}
}

void draw_entity(struct Entity* entity) {
	
	int fx = entity->frame * entity->hitbox.X;
	int fy = entity->move * entity->hitbox.Y;

	if (entity->animation != IDLE) al_draw_bitmap_region(entity->sprite, fx, fy, entity->hitbox.X, entity->hitbox.Y, entity->position.X, entity->position.Y, 0);
	else al_draw_bitmap_region(entity->sprite, 0, entity->move * entity->hitbox.Y, entity->hitbox.X, entity->hitbox.Y, entity->position.X, entity->position.Y, 0);
}

void destroy_entity(struct Entity *entity) {
	al_destroy_bitmap(entity->sprite);
}