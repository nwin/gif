#include <stdio.h>
#include <fcntl.h>

#include "gif_lib.h"

int main (int argc, char const *argv[])
{
  if (argc < 2) {
    printf("usage: read_gif <filename>\n");
    return -1;
  }
  int err = 0;
  //for (;;) {
  //GifFileType *file = DGifOpenFileName(argv[1], &err);
  GifFileType *file = DGifOpenFileHandle(open(argv[1], O_RDONLY), &err);
  if (err == GIF_OK) {
    printf("could not open the file %s\n", argv[1]);
    return -1;
  }
  DGifSlurp(file);
  printf("total image count: %d\n", file->ImageCount);
  for (int j=0; j < file->ImageCount; j++) {
    SavedImage image = file->SavedImages[j];
    printf("image %d: %d colors in palette\n", j, image.ImageDesc.ColorMap->ColorCount);
    for (int i=0; i < image.ImageDesc.Width * image.ImageDesc.Width; i++) {
      if (i%image.ImageDesc.Width == 0) {
        printf("\n");
      }
      printf("%x", image.RasterBits[i]);
    }
    printf("\n");
  }
  #if defined(GIFLIB_MAJOR) && GIFLIB_MAJOR >= 5 && GIFLIB_MINOR >= 1
  DGifCloseFile(file, &err);
  #else
  DGifCloseFile(file);
  #endif
  //}
}