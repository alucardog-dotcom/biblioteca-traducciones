/* chdman solo enlaza SDL2 por el portapapeles de la capa de MAME (ocore_sdl), que nunca usa.
   Estas funciones sustituyen a las de SDL2: portapapeles vacío. */
#include <stdlib.h>
char *SDL_GetClipboardText(void) { return calloc(1, 1); }
int SDL_HasClipboardText(void) { return 0; }
int SDL_SetClipboardText(const char *texto) { (void)texto; return -1; }
void SDL_free(void *p) { free(p); }
