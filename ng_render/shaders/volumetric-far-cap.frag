#version 450

layout(input_attachment_index = 0, set = 0, binding = 2) uniform subpassInput depth;

layout(push_constant) uniform LightBuffer {
    mat4 light_to_screen;
    vec4 sunlight_direction;
    vec4 water_transparency;
    int shadow_size;
} light_buffer;

layout(location = 0) in vec4 ndc;
layout(location = 0) out vec4 fragColor;

void main() {
    vec4 ndc2 = vec4(ndc.xy, subpassLoad(depth).r, 1);

    float facing_scale = -1.0;
    fragColor = vec4(facing_scale * vec3(1.0, 0.0, -1.0), 1.0);

    // ick... should be using a view matrix or some such to figure this out
    float near_z = 0.1;
    float d = near_z / ndc2.z; // wrong - not actual distance, oh well

    float scatter = light_buffer.water_transparency.w;
    vec3 color = scatter * -pow(light_buffer.water_transparency.xyz, vec3(d))
               / log(light_buffer.water_transparency.xyz);
    // color = vec3(1.0, 0.0, -1.0);
    // color = vec3(d);

    fragColor = vec4(facing_scale * color, 1.0);



// fragColor = vec3(0.0, 4.0, 0.0);

    // fragColor = gl_FrontFacing ? vec3(0.01, 0.02, 0.04) : vec3(0.04, 0.02, 0.02);
}
