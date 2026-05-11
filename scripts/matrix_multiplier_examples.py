import numpy as np

Q = np.array([[0.0, 0.0, 0.0, 0.0],
     [0.5, 0.0, 0.0, 0.5],
     [0.5, 0.0, 0.0, 0.5],
     [0.5, 0.0, 0.0, 0.0]])
R = np.array([[1.0, 0.0, 0.0, 0.0],
     [0.0, 0.0, 0.0, 0.0],
     [0.0, 0.0, 0.0, 0.0],
     [0.0, 0.0, 0.0, 0.5]])

I = np.eye(4)

A = np.matmul(np.linalg.inv(I - Q), R)

p = np.array([0.3, 0.0, 0.5, 0.2])

print(np.matmul(p,A))

# def apply_modifier(matrix, itinerary):
#     output = np.zeros_like(itinerary)
#     for row in range(matrix.shape[0]):
#         for col in range(matrix.shape[1]):
#             output[row] += matrix[row, col] * itinerary[col]
#     return output

# def check_communativity(matrix1, matrix2):
#     mat1 = np.matmul(matrix1, matrix2)
#     mat2 = np.matmul(matrix2, matrix1)
#     for row in range(matrix1.shape[0]):
#         for col in range(matrix1.shape[1]):
#             if abs(mat1[row, col] - mat2[row, col]) > 1e-8:  # Allow for floating-point precision issues
#                 return False
#     return True

# # Home work school community
# itinerary = np.array([0.5, 0.0, 0.3, 0.2])
# school_closure = np.array([[1.0, 0.0, 1.0, 0.0],
#                      [0.0, 1.0, 0.0, 0.0],
#                      [0.0, 0.0, 0.0, 0.0],
#                      [0.0, 0.0, 0.0, 1.0]])

# isolation = np.array([[1.0, 1.0, 1.0, 1.0, 0.0],
#                      [0.0, 0.0, 0.0, 0.0, 0.0],
#                      [0.0, 0.0, 0.0, 0.0, 0.0],
#                      [0.0, 0.0, 0.0, 0.0, 0.0],
#                      [0.0, 0.0, 0.0, 0.0, 1.0]])

# hospitalization = np.array([[0.0, 0.0, 0.0, 0.0, 0.0],
#                      [0.0, 0.0, 0.0, 0.0, 0.0],
#                      [0.0, 0.0, 0.0, 0.0, 0.0],
#                      [0.0, 0.0, 0.0, 0.0, 0.0],
#                      [1.0, 1.0, 1.0, 1.0, 1.0]])

# print("check communitivity hospitalization and isolation: {}".format(check_communativity(hospitalization, isolation)))
# print(np.matmul(hospitalization, isolation))

# shelter = np.array([[1.0, 0.5, 0.0, 0.9],
#                      [0.0, 0.5, 0.0, 0.0],
#                      [0.0, 0.0, 1.0, 0.0],
#                      [0.0, 0.0, 0.0, 0.1]])

# telework = np.array([[1.0, 1.0, 0.0, 0.0],
#                      [0.0, 0.0, 0.0, 0.0],
#                      [0.0, 0.0, 1.0, 0.0],
#                      [0.0, 0.0, 0.0, 1.0]])

# partial_isolation = np.array([[1.0, 0.9, 0.9, 0.9],
#                      [0.0, 0.1, 0.0, 0.0],
#                      [0.0, 0.0, 0.1, 0.0],
#                      [0.0, 0.0, 0.0, 0.1]])

# redistribute_community = np.array([[1.0, 0.0, 0.0, 0.3],
#                      [0.0, 1.0, 0.0, 0.3],
#                      [0.0, 0.0, 1.0, 0.2],
#                      [0.0, 0.0, 0.0, 0.2]])

# distribute_home = np.array([[0.5, 0.0, 0.0, 0.0],
#                      [0.0, 1.0, 0.0, 0.0],
#                      [0.2, 0.0, 1.0, 0.0],
#                      [0.3, 0.0, 0.0, 1.0]])

# partial_school_closure = np.array([[1.0, 0.0, 0.5, 0.0],
#                      [0.0, 1.0, 0.0, 0.0],
#                      [0.0, 0.0, 0.5, 0.0],
#                      [0.0, 0.0, 0.0, 1.0]])

# weekend_closure = np.array([[1.0, 0.5, 0.5, 0.0],
#                      [0.0, 0.0, 0.0, 0.0],
#                      [0.0, 0.0, 0.0, 0.0],
#                      [0.0, 0.5, 0.5, 1.0]])
# shelter_in_place_activity = np.array([[1.0, 0.0, 0.0, 0.9],
#                      [0.0, 1.0, 0.0, 0.0],
#                      [0.0, 0.0, 1.0, 0.0],
#                      [0.0, 0.5, 0.0, 0.1]])


# matrices = [school_closure, isolation, shelter, partial_isolation,  partial_school_closure, weekend_closure, telework]
# matrix_names = ["school_closure", "isolation", "shelter", "partial_isolation", "partial_school_closure", "weekend_closure", "telework"]
# for i in range(len(matrices)):
#     for j in range(i + 1, len(matrices)):
#         if not check_communativity(matrices[i], matrices[j]):
#             print(f"Checking commutativity between {matrix_names[i]} and {matrix_names[j]}: {check_communativity(matrices[i], matrices[j])}")

# print("default: {}".format(itinerary))
# print("telework: {}".format(apply_modifier(telework, itinerary)))
# print("weekend_closure: {}".format(apply_modifier(weekend_closure, itinerary)))
# print("telework then weekend: {}".format(apply_modifier(np.matmul(weekend_closure, telework), itinerary)))
# print("weekend then telework: {}".format(apply_modifier(np.matmul(telework, weekend_closure), itinerary)))

# print("\n")
# print("default: {}".format(itinerary))
# print("shelter: {}".format(apply_modifier(shelter, itinerary)))
# print("weekend_closure: {}".format(apply_modifier(weekend_closure, itinerary)))
# print("shelter then weekend: {}".format(apply_modifier(weekend_closure, apply_modifier(shelter, itinerary))))
# print("weekend then shelter: {}".format(apply_modifier(shelter, apply_modifier(weekend_closure, itinerary))))

# print("\n")
# print("default: {}".format(itinerary))
# print("school closure: {}".format(apply_modifier(school_closure, itinerary)))
# print("weekend_closure: {}".format(apply_modifier(weekend_closure, itinerary)))
# print("school closure then weekend: {}".format(apply_modifier(weekend_closure, apply_modifier(school_closure, itinerary))))
# print("weekend then school closure: {}".format(apply_modifier(school_closure, apply_modifier(weekend_closure, itinerary))))

# print("\n")
# print("default: {}".format(itinerary))
# print("shelter in place: {}".format(apply_modifier(shelter_in_place_activity, itinerary)))
# print("weekend_closure: {}".format(apply_modifier(weekend_closure, itinerary)))
# print("shelter in place then weekend: {}".format(apply_modifier(weekend_closure, apply_modifier(shelter_in_place_activity, itinerary))))
# print("weekend then shelter in place: {}".format(apply_modifier(shelter_in_place_activity, apply_modifier(weekend_closure, itinerary))))

# # print("weekend then shelter: {}".format(apply_modifier(np.matmul(shelter, weekend_closure), itinerary)))
# # print(np.matmul(shelter, weekend_closure))
# # school_closure_then_partial = np.matmul(school_closure, partial_school_closure)
# # partial_then_school_closure = np.matmul(partial_school_closure, school_closure)
# # # print(two_changes_isolation_closure)
# # # print(isolation_two_changes_closure)
# # # # print(isolation_school_closure)
# # # # print(apply_modifier(isolation, itinerary))
# # # # print(apply_modifier(school_closure, itinerary))
# # # print(apply_modifier(partial_isolation, itinerary))
# # # print(apply_modifier(two_changes, itinerary))
# # # print(apply_modifier(isolation_two_changes_closure, itinerary))
# # # print(apply_modifier(two_changes_isolation_closure, itinerary))
# # # # print(apply_modifier(isolation_school_closure, itinerary))
# # print(apply_modifier(school_closure, itinerary))
# # print(apply_modifier(partial_school_closure, itinerary))
# # print(school_closure_then_partial)
# # print(partial_then_school_closure)
# # print(apply_modifier(school_closure_then_partial, itinerary))
# # print(apply_modifier(partial_then_school_closure, itinerary))


