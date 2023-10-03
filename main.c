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
				
				if (sys.battle) count = 3;
				if (sys.record) count = 1;
				if (sys.configuration) count = 2;
			break;
			case 1:
				printf("record!");
				sys.record = NO;
				count = 0;
			break;
			case 2:
				printf("configuration!");
				sys.configuration = NO;
				count = 0;
			break;
			case 3: 
				do {
					result = battle(&sys);
					if (result) {
						result = 0;
					}
				} while (result && sys.running && !sys.menu);
			break;
		};
	} while (sys.running && result == 0);
}