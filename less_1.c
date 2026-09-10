#include <stdio.h>
#include <stdlib.h>

typedef struct{
	int h;
	int min;
}TicTac;

int as_minutes(TicTac a) {
    int m = (a.h * 60) + a.min;
        m = (m % 720 + 720) % 720;
    return m;
}

void adjust_to_12h(TicTac *p) {
    int m = as_minutes(*p);
    p->h   = m / 60;
    p->min = m % 60;
}

TicTac after(TicTac a, int min) {
    a.min += min;
    adjust_to_12h(&a);
    return a;
}

void forward(TicTac * me, TicTac a) {
    me->h   += a.h;
    me->min += a.min;
    adjust_to_12h(me);
}

void backward(TicTac * me, TicTac a){
    me->h   -= a.h;
    me->min -= a.min;
    adjust_to_12h(me);
}

int isEqualTime(TicTac a, TicTac b) {
    return as_minutes(a) == as_minutes(b);
}

void printTic(TicTac a) {
    printf("%02d:%02d\n", a.h, a.min);
}

int main(){
    TicTac a,b,c;
    int mk;
    scanf("%d:%d", &(a.h), &(a.min));
    scanf("%d", &mk);
    scanf("%d:%d", &(b.h), &(b.min));
    printf("equal: %d\n",isEqualTime(a,b));
    c = after(a, mk);
    printf("after: ");
    printTic(c);
    c = a;
    printf("forward: ");
    forward(&a, b);
    printTic(a);
    printf("backward: ");
    backward(&c, b);
    printTic(c);
    return 0;
}