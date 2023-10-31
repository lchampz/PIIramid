#pragma once

#define FPS 30
#define WIDTH 1280
#define HEIGHT 720

#define NAME "Piramid"

#define HOVER_BLACK al_map_rgb(92, 51, 23)
#define HOVER_WHITE al_map_rgb(255,255, 255)

#define SPRITE_CHAR_W 102 
#define SPRITE_CHAR_H 152

#define SPRITE_MUMMY_W 136
#define SPRITE_MUMMY_H 181
#define DESCRIPTION_MUMMY "Provavelmente algum farao carcumido por vermes..."
#define DESCRIPTION_MUMMY2 "apesar das mariposas acharem suas ataduras apetitosas, seu maior medo e a"
#define DESCRIPTION_ZOMBIE "Ceeeeeeeerebros... Ceeeeeeeeeeerebros..."
#define DESCRIPTION_ZOMBIE2 "Um defunto sedento por cerebros, um pouco lento, fedido e bem burro."
#define DESCRIPTION_ZOMBIE3 "Deve ser por isso que seu maior medo e a SOMA"

#define INTRO "Seu nome e Tony, um explorador avido por novas descober-"
#define INTRO2 "tas. Apos uma carta misteriosa do seu tio, seu espirito de"
#define INTRO3 "aventura falou mais alto e te levou a uma jornada"
#define INTRO4 "desconhecida no egito antigo..."
#define INTRO5 "Voce esta preparado?"

typedef struct {
	int X;
	int Y;
} Coordenades;

typedef struct {
	Coordenades final;
	bool done;
	Coordenades actual;
} Animation;

enum YesOrNo {
	NO, YES
} YesOrNo;