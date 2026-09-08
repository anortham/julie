#include <stdio.h>

struct Point {
    int x;
    int y;
};

int compute_area(struct Point* pt) {
    if (!pt) {
        return 0;
    }
    return pt->x * pt->y;
}
