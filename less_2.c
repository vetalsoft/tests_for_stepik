// #include <stdio.h>
// #include <math.h>

typedef struct {
    int x;
    int y;
} Point;

typedef struct {
    Point a;
    Point b;
    float len;
} Line;

float distance(Point a, Point b) {
    int dx = b.x - a.x;
    int dy = b.y - a.y;
    return sqrt(dx * dx + dy * dy);
}

void scanLine(Line *t) {
    scanf("%d %d %d %d", &(t->a.x), &(t->a.y), &(t->b.x), &(t->b.y));
    t->len = distance(t->a, t->b);
}

void printLine(Line t) {
    printf("%d %d %d %d %.3f\n", t.a.x, t.a.y, t.b.x, t.b.y, t.len);
}

void rotRLine(Line *t) {
    Point new_a;
    Point new_b;
    new_a.x = t->a.y;
    new_a.y = -(t->a.x);
    new_b.x = t->b.y;
    new_b.y = -(t->b.x);
    t->a = new_a;
    t->b = new_b;
}

int main() {
    Line t;
    scanLine(&t);
    rotRLine(&t);
    printLine(t);
    return 0;
}