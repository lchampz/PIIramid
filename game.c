#define _CRT_SECURE_NO_DEPRECATE
#include "game.h"
#include <string.h>

void destroyBtn(Button btn) {
	if (btn.font != NULL) al_destroy_font(btn.font);
	if (btn.bitmap != NULL) al_destroy_bitmap(btn.bitmap);
}

bool init_map(Map* map) {
	char c;
	char memo[10] = {" "};
	char a[10] = {" "};
	bool exec[2] = { YES, NO };
	FILE* arq = fopen(map->path, "r");
	printf("[LOG] abrindo arquivo...\n");
	
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
			map->rows = strtol(a, NULL, 10);
			memset(memo, 0, 10);
			memset(a, 0, 10);
			exec[0] = NO;
			exec[1] = YES;
			i = 0;
		}

		if (exec[1] && i == 1) {
			strcat(a, memo);
			map->columns = strtol(a, NULL, 10);
			exec[1] = NO;
		}
	}

	printf("[LOG] %d linhas!\n", map->rows);
	printf("[LOG] %d colunas!\n", map->columns);
	fclose(arq);
	map->finish = YES;
}