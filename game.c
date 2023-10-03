#define _CRT_SECURE_NO_DEPRECATE
#include "game.h"
#include <string.h>

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

bool init_map(Map* map) {
	char c;
	char memo[10] = {" "};
	char a[10] = {" "};
	bool exec[2] = { YES, NO };
	FILE* arq = fopen(map->path, "r");
	printf("[LOG] abrindo arquivo...\n");

	struct Matrix {
		int column;
		int row;
	};
	
	if (arq == NULL) {
		map->finish = YES;
		printf("[ERRO] Falha em abrir o arquivo!\n");
		return NO;
	}

	printf("[LOG] lendo linha e coluna...\n");
	for (int i = 0; 4 > i; i++) {
		c = fgetc(arq);
		if (c == '\n') i -= 2;
		else memo[i] = c;


		if (exec[0] && i == 1) {
			strcat(a, memo);
			map->columns = strtol(a, NULL, 10);
			memset(memo, 0, 10);
			memset(a, 0, 10);
			exec[0] = NO;
			exec[1] = YES;
			i = 0;
		}

		if (exec[1] && i == 1) {
			strcat(a, memo);
			map->rows = strtol(a, NULL, 10);
			exec[1] = NO;
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
	for (int i = 0; map->rows > i; i++) arr[i] = calloc(map->columns, sizeof(int)); //alocando espaço variavel em cada celula
	
	if (arr == NULL) printf("[ERRO] Não foi possível alocar memória!");

	for (int i = 0; matrix.row > i; i++) {
			for (int j = 0; matrix.column > j; j++) {
				c = fgetc(arq);
				if (c != ' ' && c != "\n") arr[i][j] = asciiNumbers(c);
			}
	}

	for (int i = 0; matrix.row > i; i++) {
		for (int j = 0; matrix.column > j; j++) {
			printf("%d ", arr[i][j]);
		}
		printf("\n");
	}

	for (int i = 0; matrix.row > i; i++) free(arr[i]);
	free(arr);

	fclose(arq);
	map->finish = YES;
}