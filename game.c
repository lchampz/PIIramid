#include "game.h"

void destroyBtn(Button btn) {
	if (btn.font != NULL) al_destroy_font(btn.font);
	if (btn.bitmap != NULL) al_destroy_bitmap(btn.bitmap);
}