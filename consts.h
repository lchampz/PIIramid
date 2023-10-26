#pragma once

#define FPS 30
#define WIDTH 1280
#define HEIGHT 720

#define NAME "Piramid"

#define HOVER_BLACK al_map_rgb(92, 51, 23)
#define HOVER_WHITE al_map_rgb(255,255, 255)

#define SPRITE_CHAR_W 102 
#define SPRITE_CHAR_H 152
#define DESCRIPTION_MUMMY "Provavelmente algum farao carcumido por vermes..."
#define DESCRIPTION_MUMMY2 "apesar das mariposas acharem suas ataduras apetitosas, seu maior medo e a"

typedef struct {
	int X;
	int Y;
} Coordenades;

typedef struct {
	Coordenades initial;
	Coordenades final;
	bool done;
} Animation;

enum YesOrNo {
	NO, YES
} YesOrNo;