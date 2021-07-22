#version 450

layout(push_constant) uniform LightBuffer {
    mat4 light_to_screen;
    vec4 sunlight_direction;
    vec4 water_transparency;
    int shadow_size;
} light_buffer;

layout(location = 0) in vec4 ndc;
layout(location = 0) out vec4 fragColor;

void main() {
    float facing_scale = 2*float(gl_FrontFacing) - 1.0;

    vec3 ndc2 = ndc.xyz / ndc.w;
    // ick... should be using a view matrix or some such to figure this out
    float near_z = 0.1;
    float d = near_z / ndc2.z; // wrong - not actual distance, oh well

    float scatter = light_buffer.water_transparency.w;
    vec3 color = scatter * -pow(light_buffer.water_transparency.xyz, vec3(d))
               / log(light_buffer.water_transparency.xyz);
    // color = vec3(1.0, 0.0, -1.0);
    // color = vec3(d);

    fragColor = vec4(facing_scale * color, 1.0);

    // fragColor = gl_FrontFacing ? vec4(0.01, 0.02, 0.04, 1.0) : vec4(0.04, 0.02, 0.02, 1.0);
}
