#include "game.h"

int main(void) {
	System sys = init();

	if (sys.error == YES) return -1;

	int result;
	int count = 0;

	do {
		switch (count)
		{
			case 0:
				result = menu(&sys);
				if (sys.record) //count = 3;
				if (sys.configuration) //count = 4;
			break;
			case 3:
				printf("record!");
				count = 0;
			break;
			case 4: 
				printf("config!");
				count = 0;
			break;
		};
	} while (sys.running && result == 0);
}