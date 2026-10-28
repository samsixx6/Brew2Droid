/* Adaptador e primitivas derivadas do FFmpeg n8.0, LGPL-2.1-or-later.
 * Copyrights e fontes integrais em upstream/; ver README.md e COPYING.LGPLv2.1.
 * Não incorpora a implementação de referência proprietária IS-733. */

#ifndef BREW_QCELP_SUPPORT_H
#define BREW_QCELP_SUPPORT_H
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <assert.h>
#include <inttypes.h>
#define FFMAX(a,b) ((a) > (b) ? (a) : (b))
#define FFMIN(a,b) ((a) < (b) ? (a) : (b))
#define FFABS(a) ((a) >= 0 ? (a) : -(a))
#define FF_ARRAY_ELEMS(a) (sizeof(a) / sizeof((a)[0]))
#define FFSWAP(t,a,b) do { t tmp = (a); (a) = (b); (b) = tmp; } while (0)
#define av_assert2(a) assert(a)
#define av_cold
#define AV_LOG_WARNING 0
#define AV_LOG_ERROR 0
#define av_log(...) ((void)0)
#define avpriv_request_sample(...) ((void)0)
#define AV_RB16(p) (((unsigned)(p)[0] << 8) | (p)[1])
#define MAX_LP_HALF_ORDER 10
#ifndef M_PI
#define M_PI 3.14159265358979323846
#endif
static int av_clip(int value, int low, int high) { return FFMIN(FFMAX(value, low), high); }
typedef struct { const uint8_t *data; size_t bits, at; } GetBitContext;
static int init_get_bits8(GetBitContext *s, const uint8_t *data, int size) {
    if (size < 0) return -1;
    s->data=data; s->bits=(size_t)size*8; s->at=0; return 0;
}
static unsigned get_bits(GetBitContext *s, unsigned count) {
    unsigned result=0;
    for (unsigned i=0; i<count; i++,s->at++) {
        result <<= 1;
        if (s->at < s->bits) result |= (s->data[s->at/8] >> (7-s->at%8)) & 1;
    }
    return result;
}
typedef struct { void *priv_data; int64_t frame_num; } AVCodecContext;
typedef struct { uint8_t *data[1]; int nb_samples; } AVFrame;
typedef struct { const uint8_t *data; int size; } AVPacket;
static int ff_get_buffer(AVCodecContext *ctx, AVFrame *frame, int flags) {
    (void)ctx; (void)frame; (void)flags; return 0;
}
static float ff_scalarproduct_float_c(const float *a,const float *b,int n) {
    float sum=0; for (int i=0;i<n;i++) sum+=a[i]*b[i]; return sum;
}

/* acelp_vectors.c */
void ff_weighted_vector_sumf(float *out, const float *in_a, const float *in_b,
                             float weight_coeff_a, float weight_coeff_b, int length)
{
    int i;

    for(i=0; i<length; i++)
        out[i] = weight_coeff_a * in_a[i]
               + weight_coeff_b * in_b[i];
}

/* acelp_vectors.c */
void ff_adaptive_gain_control(float *out, const float *in, float speech_energ,
                              int size, float alpha, float *gain_mem)
{
    int i;
    float postfilter_energ = ff_scalarproduct_float_c(in, in, size);
    float gain_scale_factor = 1.0;
    float mem = *gain_mem;

    if (postfilter_energ)
        gain_scale_factor = sqrt(speech_energ / postfilter_energ);

    gain_scale_factor *= 1.0 - alpha;

    for (i = 0; i < size; i++) {
        mem = alpha * mem + gain_scale_factor;
        out[i] = in[i] * mem;
    }

    *gain_mem = mem;
}

/* acelp_vectors.c */
void ff_scale_vector_to_given_sum_of_squares(float *out, const float *in,
                                             float sum_of_squares, const int n)
{
    int i;
    float scalefactor = ff_scalarproduct_float_c(in, in, n);
    if (scalefactor)
        scalefactor = sqrt(sum_of_squares / scalefactor);
    for (i = 0; i < n; i++)
        out[i] = in[i] * scalefactor;
}

/* acelp_filters.c */
void ff_tilt_compensation(float *mem, float tilt, float *samples, int size)
{
    float new_tilt_mem = samples[size - 1];
    int i;

    for (i = size - 1; i > 0; i--)
        samples[i] -= tilt * samples[i - 1];

    samples[0] -= tilt * *mem;
    *mem = new_tilt_mem;
}

/* celp_filters.c */
void ff_celp_lp_synthesis_filterf(float *out, const float *filter_coeffs,
                                  const float* in, int buffer_length,
                                  int filter_length)
{
    int i,n;

#if 0 // Unoptimized code path for improved readability
    for (n = 0; n < buffer_length; n++) {
        out[n] = in[n];
        for (i = 1; i <= filter_length; i++)
            out[n] -= filter_coeffs[i-1] * out[n-i];
    }
#else
    float out0, out1, out2, out3;
    float old_out0, old_out1, old_out2, old_out3;
    float a,b,c;

    a = filter_coeffs[0];
    b = filter_coeffs[1];
    c = filter_coeffs[2];
    b -= filter_coeffs[0] * filter_coeffs[0];
    c -= filter_coeffs[1] * filter_coeffs[0];
    c -= filter_coeffs[0] * b;

    av_assert2((filter_length&1)==0 && filter_length>=4);

    old_out0 = out[-4];
    old_out1 = out[-3];
    old_out2 = out[-2];
    old_out3 = out[-1];
    for (n = 0; n <= buffer_length - 4; n+=4) {
        float tmp0,tmp1,tmp2;
        float val;

        out0 = in[0];
        out1 = in[1];
        out2 = in[2];
        out3 = in[3];

        out0 -= filter_coeffs[2] * old_out1;
        out1 -= filter_coeffs[2] * old_out2;
        out2 -= filter_coeffs[2] * old_out3;

        out0 -= filter_coeffs[1] * old_out2;
        out1 -= filter_coeffs[1] * old_out3;

        out0 -= filter_coeffs[0] * old_out3;

        val = filter_coeffs[3];

        out0 -= val * old_out0;
        out1 -= val * old_out1;
        out2 -= val * old_out2;
        out3 -= val * old_out3;

        for (i = 5; i < filter_length; i += 2) {
            old_out3 = out[-i];
            val = filter_coeffs[i-1];

            out0 -= val * old_out3;
            out1 -= val * old_out0;
            out2 -= val * old_out1;
            out3 -= val * old_out2;

            old_out2 = out[-i-1];

            val = filter_coeffs[i];

            out0 -= val * old_out2;
            out1 -= val * old_out3;
            out2 -= val * old_out0;
            out3 -= val * old_out1;

            FFSWAP(float, old_out0, old_out2);
            old_out1 = old_out3;
        }

        tmp0 = out0;
        tmp1 = out1;
        tmp2 = out2;

        out3 -= a * tmp2;
        out2 -= a * tmp1;
        out1 -= a * tmp0;

        out3 -= b * tmp1;
        out2 -= b * tmp0;

        out3 -= c * tmp0;


        out[0] = out0;
        out[1] = out1;
        out[2] = out2;
        out[3] = out3;

        old_out0 = out0;
        old_out1 = out1;
        old_out2 = out2;
        old_out3 = out3;

        out += 4;
        in  += 4;
    }

    out -= n;
    in -= n;
    for (; n < buffer_length; n++) {
        out[n] = in[n];
        for (i = 1; i <= filter_length; i++)
            out[n] -= filter_coeffs[i-1] * out[n-i];
    }
#endif
}

/* celp_filters.c */
void ff_celp_lp_zero_synthesis_filterf(float *out, const float *filter_coeffs,
                                       const float *in, int buffer_length,
                                       int filter_length)
{
    int i,n;

    for (n = 0; n < buffer_length; n++) {
        out[n] = in[n];
        for (i = 1; i <= filter_length; i++)
            out[n] += filter_coeffs[i-1] * in[n-i];
    }
}

/* lsp.c */
static void lsp2polyf(const double *lsp, double *f, int lp_half_order)
{
    f[0] = 1.0;
    f[1] = -2 * lsp[0];
    for (int i = 2; i <= lp_half_order; i++) {
        double val = -2 * lsp[2*i-2];
        f[i] = val * f[i-1] + 2*f[i-2];
        for (int j = i-1; j > 1; j--)
            f[j] += f[j-1] * val + f[j-2];
        f[1] += val;
    }
}

/* lsp.c */
void ff_acelp_lspd2lpc(const double *lsp, float *lpc, int lp_half_order)
{
    double pa[MAX_LP_HALF_ORDER+1], qa[MAX_LP_HALF_ORDER+1];
    float *lpc2 = lpc + (lp_half_order << 1) - 1;

    av_assert2(lp_half_order <= MAX_LP_HALF_ORDER);

    lsp2polyf(lsp,     pa, lp_half_order);
    lsp2polyf(lsp + 1, qa, lp_half_order);

    while (lp_half_order--) {
        double paf = pa[lp_half_order+1] + pa[lp_half_order];
        double qaf = qa[lp_half_order+1] - qa[lp_half_order];

        lpc [ lp_half_order] = 0.5*(paf+qaf);
        lpc2[-lp_half_order] = 0.5*(paf-qaf);
    }
}

#endif

