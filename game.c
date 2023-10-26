#define _CRT_SECURE_NO_DEPRECATE
#include "game.h"
#include <string.h>

int binarySearch(int arr[], int num, int left, int right) {
	while (right > left) {
		int m = left + (right - left / 2);

		if (arr[m] == num)
			return m;

		printf("posicao = %d\n", m);

		if (arr[m] < num)
			return binarySearch(arr, num, m + 1, right - 1);

		return binarySearch(arr, num, left, m - 1);
	}
	return -1;
}

void drawBtn(Button btn) {
	al_draw_bitmap(btn.bitmap, btn.coordenades.X, btn.coordenades.Y, 0);
	if (btn.isHover) al_draw_text(btn.font, HOVER_WHITE, btn.coordenades.X + btn.fontPosition.X, btn.coordenades.Y + +btn.fontPosition.Y, 0, btn.placeholder);
	else al_draw_text(btn.font, HOVER_BLACK, btn.coordenades.X + btn.fontPosition.X, btn.coordenades.Y + btn.fontPosition.Y, 0, btn.placeholder);
}

void destroyBtn(Button btn) {
	if (btn.font != NULL) al_destroy_font(btn.font);
	if (btn.bitmap != NULL) al_destroy_bitmap(btn.bitmap);
}

int asciiNumbers(int num) {
	switch (num) {
	case 48:
		return 0;
		break;
	case 49:
		return 1;
		break;
	case 50:
		return 2;
		break;
	case 51:
		return 3;
		break;
	case 52:
		return 4;
		break;
	case 53:
		return 5;
		break;
	case 54:
		return 6;
		break;
	case 55:
		return 7;
		break;
	case 56:
		return 8;
		break;
	case 57:
		return 9;
		break;
	}
}

void draw_map(struct Map* map) {
	if (map->tileArr == NULL) {
		printf("[ERRO] Mapa não localizado!");
		map->error = YES;	
	}
	if (map->error) return;
	for (int i = 0; map->rows > i; i++) {
		for (int j = 0; map->columns > j; j++) {
			al_draw_bitmap(map->tileArr[i][j].path, map->tileArr[i][j].position.X, map->tileArr[i][j].position.Y, 0);
		}
	}
}

void init_map(struct Map* map) {
	char c;
	char memo[10] = {" "};
	char a[10] = {" "};
	FILE* arq = fopen(map->path, "r");
	printf("[LOG] abrindo arquivo...\n");

	struct Matrix {
		int column;
		int row;
	};
	
	if (arq == NULL) {
		map->finish = YES;
		printf("[ERRO] Falha em abrir o arquivo!\n");
		map->error = YES;
		return;
	}

	printf("[LOG] lendo linha e coluna...\n");
	for (int i = 0; 4 > i; i++) {
		c = fgetc(arq);
		if (i >= 2) {
			memo[i - 2] = c;
		} else memo[i] = c;

		if (i == 1) {
			strcat(a, memo);
			map->columns = strtol(a, NULL, 10);
			memset(memo, 0, 10);
			memset(a, 0, 10);
		}

		if (i == 3) {
			strcat(a, memo);
			map->rows = strtol(a, NULL, 10);
		}
	}

	printf("[LOG] %d linhas!\n", map->rows);
	printf("[LOG] %d colunas!\n", map->columns);

	int **arr;

	struct Matrix matrix = {
		.column = map->columns,
		.row = map->rows,
	};

	arr = malloc(map->rows * sizeof(int*)); //alocando espaço variavel na memória 
	for (int i = 0; map->rows > i; i++) arr[i] = malloc(map->columns * sizeof(int)); //alocando espaço variavel em cada celula
	
	if (arr == NULL) {
		printf("[ERRO] Não foi possível alocar memória!");
		map->error = YES;
	}

	for (int i = 0; matrix.row > i; i++) {
			for (int j = 0; matrix.column > j; j++) {
				c = fgetc(arq);
				if (c != ' ' && c != "\n" && c != EOF) arr[i][j] = asciiNumbers(c);
				else break;
			}
	}

	Tile** tileArr;
	tileArr = malloc(map->rows * sizeof(Tile*));
	for (int i = 0; map->rows > i; i++) tileArr[i] = malloc(map->columns * sizeof(Tile));
	Tile floor = { .type = FLOOR, .path = al_load_bitmap("./assets/tileset/chao1.png"), .hasColision = NO };
	Tile wall = { .type = WALL, .path = al_load_bitmap("./assets/tileset/muro1.png"), .hasColision = YES };

	for (int i = 0; matrix.row > i; i++) {
		for (int j = 0; matrix.column > j; j++) {
			switch (arr[i][j]) {
				case FLOOR:
					floor.position.X = 32 * i;
					floor.position.Y = 32 * j;

					tileArr[i][j] = floor;
				break;
				case WALL:
					wall.position.X = 32 * i;
					wall.position.Y = 32 * j;

					tileArr[i][j] = wall;
				break;
			}
		}
	}

	map->tileArr = tileArr;

	for (int i = 0; map->rows > i; i++) {
		for (int j = 0; map->columns > j; j++) {
			printf("%d ", arr[i][j]);
		}
		printf("\n");
	}

	for (int i = 0; matrix.row > i; i++) free(arr[i]);
	free(arr);

	fclose(arq);
	map->finish = YES;
}

