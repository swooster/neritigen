#version 450

layout(input_attachment_index = 0, set = 0, binding = 0) uniform subpassInput diffuse;
layout(input_attachment_index = 0, set = 0, binding = 1) uniform subpassInput normal;
layout(input_attachment_index = 0, set = 0, binding = 2) uniform subpassInput depth;
layout(set = 0, binding = 3) uniform sampler2D shadow;

layout(push_constant) uniform LightBuffer {
    mat4 screen_to_light;
    vec4 sunlight_direction;
    vec4 water_transparency;
    int shadow_size;
} light_buffer;

layout(location = 0) in vec4 ndc;
layout(location = 0) out vec3 fragColor;

void main() {
    vec4 position_in_light = light_buffer.screen_to_light * vec4(ndc.xy, subpassLoad(depth).r, 1);
    vec2 shadow_coords = 0.5 * position_in_light.xy / position_in_light.w + vec2(0.5);
    float geometry_depth = position_in_light.z / position_in_light.w;
    float shadow_depth = texture(shadow, shadow_coords, 0.0).r;
    float shadow_threshold_narrowness = 1024;
    float shadow_factor = 1 - clamp(shadow_threshold_narrowness * (shadow_depth - geometry_depth), 0, 1);

    float cosine_factor = clamp(-dot(light_buffer.sunlight_direction.xyz, 2 * subpassLoad(normal).rgb - vec3(1)), 0, 1);

    vec3 ndc2 = vec3(ndc.xy, subpassLoad(depth).r);
    // ick... should be using a view matrix or some such to figure this out
    float near_z = 0.1;
    float d = near_z / ndc2.z; // wrong - not actual distance, oh well
    vec3 water_absorption_factor = pow(light_buffer.water_transparency.xyz, vec3(d));

    fragColor = (0.95 * shadow_factor * cosine_factor + 0.05) * water_absorption_factor * subpassLoad(diffuse).rgb;
}
